//! Stage 4: cross-file edge resolution.
//!
//! Runs inside the same transaction as stages 1–3. Builds an in-memory name
//! index from the graph (SQLite read-your-own-writes sees the uncommitted nodes
//! from stages 1–3 on the same connection), then emits `Calls` and `Imports`
//! edges for newly parsed files. The caller owns the transaction; this module
//! never calls begin or commit.
//!
//! ## Approximate resolution contract (phase 1)
//!
//! - Call-site names are matched by the final component of the symbol's
//!   qualified name. Ambiguous matches emit edges to all candidates.
//! - Import paths are resolved for TypeScript (relative `./` and `../` paths),
//!   Python (dotted-name to slash-path), Rust (`crate::` / `self::` / `super::`
//!   plus crate-relative first segments → `.rs` / `mod.rs` candidates), and Go
//!   (`go.mod` prefix stripping with per-directory `.go` fan-out).
//! - Unresolved names are silently skipped — no error, no placeholder edge.
//! - Cross-file edges from unchanged files are preserved from the previous cycle
//!   because the delete cascade on changed file nodes cleans up stale edges.
//!
//! ## Resolver lookups use the graph's `file_index`, not the filesystem
//!
//! Rust top-level-name checks and Go package fan-out both enumerate files via
//! the in-memory `file_index` / `files_by_dir` built from `all_file_paths()`.
//! This guarantees the resolver's view matches the graph (respecting
//! `.gitignore` and redactions) and avoids one syscall per import.

use std::{
    collections::HashMap,
    fs,
    path::{Component, Path, PathBuf},
};

use crate::{
    core::ids::{FileNodeId, NodeId, SymbolNodeId},
    pipeline::structural::ids::derive_edge_id,
    pipeline::structural::provenance::make_provenance,
    structure::{
        graph::{Edge, EdgeKind, Epistemic, GraphStore},
        parse::{ExtractedCallRef, ExtractedImportRef, Language},
    },
};

/// Per-compile resolver state threaded into every import reference.
///
/// Built once at the top of `run_cross_file_resolution` so stage 4 does not
/// re-read `go.mod`, re-walk `Cargo.toml`, or re-scan package directories per
/// import_ref.
pub(super) struct ResolverContext {
    pub(super) repo_root: PathBuf,
    /// Every file the graph knows about, keyed by repo-relative path
    /// (forward-slash separators on all platforms).
    pub(super) file_index: HashMap<String, FileNodeId>,
    /// Files grouped by parent directory. The empty string key holds
    /// repo-root files. Used for O(1) "directory exists" checks and Go
    /// package fan-out without a filesystem walk.
    pub(super) files_by_dir: HashMap<String, Vec<String>>,
    /// `module …` line from `<repo_root>/go.mod`, or `None`.
    pub(super) go_module_prefix: Option<String>,
    /// `go_module_prefix` with a trailing `/`, precomputed so per-import prefix
    /// stripping does not allocate.
    pub(super) go_module_prefix_slash: Option<String>,
    /// `rust_crate_src` result keyed by the importing file's parent directory
    /// (absolute path, built via `repo_root.join(importing_file).parent()`).
    /// Populated up-front for every Rust file in `pending`; all other Rust
    /// files inside a walked dir reuse the cached answer.
    pub(super) rust_crate_src_by_dir: HashMap<PathBuf, Option<Vec<String>>>,
}

/// Read `<repo_root>/go.mod` and return the declared module prefix.
///
/// Scans for the first whitespace-trimmed line starting with `module ` and
/// returns the remainder. Returns `None` when the file is missing, unreadable,
/// or does not declare a module line (e.g., a commented-out stub).
pub(super) fn load_go_module_prefix(repo_root: &Path) -> Option<String> {
    let contents = fs::read_to_string(repo_root.join("go.mod")).ok()?;
    for line in contents.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("module ") {
            let prefix = rest.trim();
            if !prefix.is_empty() {
                // Go module paths may appear bare or inside quotes; strip either.
                let unquoted = prefix.trim_matches('"');
                return Some(unquoted.to_string());
            }
        }
    }
    None
}

/// Pending cross-file resolution work for one file parsed this cycle.
pub struct CrossFilePending {
    pub file_id: FileNodeId,
    pub file_path: String,
    pub call_refs: Vec<ExtractedCallRef>,
    pub import_refs: Vec<ExtractedImportRef>,
}

/// Run stage 4: build the global name/file index and emit cross-file edges.
///
/// Returns the number of new edges emitted.
pub fn run_cross_file_resolution(
    graph: &mut dyn GraphStore,
    pending: &[CrossFilePending],
    revision: &str,
    repo_root: &Path,
) -> crate::Result<usize> {
    if pending.is_empty() {
        return Ok(0);
    }

    // Build name index from all symbols currently in the graph.
    // Key: short name (last '::' component of qualified_name).
    // Value: all symbol IDs with that short name.
    //
    // These reads run inside the caller's open transaction. SQLite guarantees
    // that a connection sees its own uncommitted writes, so this sees nodes
    // inserted by stages 1–3 even though they haven't been committed yet.
    let all_symbols = graph.all_symbol_names()?;
    let mut name_index: HashMap<String, Vec<SymbolNodeId>> = HashMap::new();
    for (sym_id, _file_id, qname) in &all_symbols {
        let short = qname.rsplit("::").next().unwrap_or(qname.as_str());
        // Avoid allocating a fresh key on every hit (~1 String per symbol,
        // across 100k-symbol graphs this is measurable).
        if let Some(v) = name_index.get_mut(short) {
            v.push(*sym_id);
        } else {
            name_index.insert(short.to_string(), vec![*sym_id]);
        }
    }

    // Build file_index and files_by_dir in a single pass so both share the
    // same allocation and enumerate the same set.
    let all_files = graph.all_file_paths()?;
    let mut file_index: HashMap<String, FileNodeId> = HashMap::with_capacity(all_files.len());
    let mut files_by_dir: HashMap<String, Vec<String>> = HashMap::new();
    for (path, file_id) in all_files {
        match path.rsplit_once('/') {
            Some((dir, file)) => {
                files_by_dir
                    .entry(dir.to_string())
                    .or_default()
                    .push(file.to_string());
            }
            None => {
                files_by_dir
                    .entry(String::new())
                    .or_default()
                    .push(path.clone());
            }
        }
        file_index.insert(path, file_id);
    }

    let go_module_prefix = load_go_module_prefix(repo_root);
    let go_module_prefix_slash = go_module_prefix.as_deref().map(|p| format!("{p}/"));

    // Precompute Rust `rust_crate_src` per unique parent directory of pending
    // `.rs` files. `rust_crate_src` walks up the filesystem looking for
    // `Cargo.toml`, so deduplicating by parent dir turns O(files × depth)
    // syscalls into O(unique_dirs × depth).
    let mut rust_crate_src_by_dir: HashMap<PathBuf, Option<Vec<String>>> = HashMap::new();
    for item in pending {
        if !item.file_path.ends_with(".rs") {
            continue;
        }
        let importing_abs = repo_root.join(&item.file_path);
        if let Some(parent) = importing_abs.parent() {
            if !rust_crate_src_by_dir.contains_key(parent) {
                let src = rust_crate_src_walk(repo_root, parent);
                rust_crate_src_by_dir.insert(parent.to_path_buf(), src);
            }
        }
    }

    let ctx = ResolverContext {
        repo_root: repo_root.to_path_buf(),
        file_index,
        files_by_dir,
        go_module_prefix,
        go_module_prefix_slash,
        rust_crate_src_by_dir,
    };

    // Edge insertions run inside the caller's open transaction; no begin/commit here.
    let mut emitted = 0usize;

    for item in pending {
        // Calls edges: file → callee symbol
        for call_ref in &item.call_refs {
            let candidates = name_index
                .get(&call_ref.callee_name)
                .map(Vec::as_slice)
                .unwrap_or(&[]);

            for &callee_id in candidates {
                let edge = Edge {
                    id: derive_edge_id(
                        NodeId::File(item.file_id),
                        NodeId::Symbol(callee_id),
                        EdgeKind::Calls,
                    ),
                    from: NodeId::File(item.file_id),
                    to: NodeId::Symbol(callee_id),
                    kind: EdgeKind::Calls,
                    owner_file_id: None,
                    last_observed_rev: None,
                    retired_at_rev: None,
                    epistemic: Epistemic::ParserObserved,
                    drift_score: 0.0,
                    provenance: make_provenance("stage4_calls", revision, &item.file_path, ""),
                };
                graph.insert_edge(edge)?;
                emitted += 1;
            }
        }

        // Imports edges: file → imported file.
        //
        // Dispatch per importing-file language:
        // - Go imports legitimately fan out across every `.go` file in the
        //   target package directory; the resolver returns all of them and
        //   the caller emits an edge for each one that exists.
        // - For every other language, the resolver returns candidates in
        //   preference order (e.g., Rust puts `<base>.rs` before the sub-item
        //   `<base_without_last>.rs` fallback), and the caller emits an edge
        //   only for the first candidate that exists in `file_index`.
        //
        // Duplicate `(from, to, kind)` inserts collapse via `derive_edge_id`
        // + `ON CONFLICT(id) DO UPDATE` in the sqlite store, so this is
        // idempotent regardless of dispatch — the per-language split here
        // preserves TS/Python's "first match wins" and Rust's "longest-match"
        // semantics without relying on that.
        let importing_lang = Path::new(&item.file_path)
            .extension()
            .and_then(|e| e.to_str())
            .and_then(Language::from_extension);
        for import_ref in &item.import_refs {
            let candidates = resolve_import_ref(&import_ref.module_ref, &item.file_path, &ctx);
            let targets: Vec<FileNodeId> = if importing_lang == Some(Language::Go) {
                candidates
                    .into_iter()
                    .filter_map(|p| ctx.file_index.get(&p).copied())
                    .collect()
            } else {
                candidates
                    .into_iter()
                    .find_map(|p| ctx.file_index.get(&p).copied())
                    .into_iter()
                    .collect()
            };
            for target_id in targets {
                if target_id == item.file_id {
                    continue; // skip self-import
                }
                let edge = Edge {
                    id: derive_edge_id(
                        NodeId::File(item.file_id),
                        NodeId::File(target_id),
                        EdgeKind::Imports,
                    ),
                    from: NodeId::File(item.file_id),
                    to: NodeId::File(target_id),
                    kind: EdgeKind::Imports,
                    owner_file_id: None,
                    last_observed_rev: None,
                    retired_at_rev: None,
                    epistemic: Epistemic::ParserObserved,
                    drift_score: 0.0,
                    provenance: make_provenance("stage4_imports", revision, &item.file_path, ""),
                };
                graph.insert_edge(edge)?;
                emitted += 1;
            }
        }
    }

    Ok(emitted)
}

/// Attempt to resolve a module reference to one or more repo-relative file paths.
///
/// Returns every candidate worth looking up in `file_index`. The caller drops
/// candidates that are absent, so returning extras is cheap. Dispatch is keyed
/// primarily on the importing file's language, so each language's resolver is
/// only invoked when the module_ref came from a compatible parser. Handles:
/// - TypeScript/JavaScript relative imports (`./foo`, `../bar/baz`).
/// - Rust `use` paths (`crate::`, `self::`, `super::`, plus crate-relative
///   bare first segments) mapped to `.rs` / `mod.rs` candidates.
/// - Python dotted imports (`foo.bar` → `foo/bar.py`).
/// - Go imports whose prefix matches the local `go.mod` module declaration,
///   fanned out across every `.go` file in the resolved package directory.
fn resolve_import_ref(
    module_ref: &str,
    importing_file: &str,
    ctx: &ResolverContext,
) -> Vec<String> {
    if module_ref.is_empty() {
        return vec![];
    }

    let importing_lang = Path::new(importing_file)
        .extension()
        .and_then(|e| e.to_str())
        .and_then(Language::from_extension);

    match importing_lang {
        Some(Language::Rust) => return resolve_rust_use(module_ref, importing_file, ctx),
        Some(Language::Go) => {
            // `interpreted_string_literal` captures include the surrounding quotes.
            let stripped = module_ref
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .unwrap_or(module_ref);
            return resolve_go_import(stripped, ctx);
        }
        _ => {}
    }

    // TypeScript / JavaScript relative imports: ./foo  ../bar/baz
    if module_ref.starts_with("./") || module_ref.starts_with("../") {
        let Some(dir) = Path::new(importing_file).parent() else {
            return vec![];
        };
        let joined = dir.join(module_ref);
        let normalized = normalize_path(&joined);
        let base_owned;
        // Graph paths use forward slashes on all platforms; Path::join uses the
        // OS separator on Windows, so normalize before matching.
        let base = if cfg!(windows) {
            let Some(norm) = normalized.to_str() else {
                return vec![];
            };
            base_owned = norm.replace('\\', "/");
            base_owned.as_str()
        } else {
            let Some(norm) = normalized.to_str() else {
                return vec![];
            };
            norm
        };

        let mut candidates = Vec::new();
        // Try bare path + common extensions
        for ext in &["ts", "tsx", "js", "jsx", "mts", "cts"] {
            let candidate = format!("{base}.{ext}");
            if !candidate.contains("..") {
                candidates.push(candidate);
            }
        }
        // Try index file inside the directory
        for ext in &["ts", "tsx", "js"] {
            let candidate = format!("{base}/index.{ext}");
            if !candidate.contains("..") {
                candidates.push(candidate);
            }
        }
        return candidates;
    }

    // Python dotted import: foo.bar → foo/bar.py
    // Only attempt for simple top-level names (no leading dot = relative).
    if !module_ref.starts_with('.')
        && !module_ref.contains('/')
        && module_ref
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
    {
        let slash_path = module_ref.replace('.', "/");
        let candidate = format!("{slash_path}.py");
        return vec![candidate];
    }

    vec![]
}

/// Resolve a Rust `use` path to candidate repo-relative `.rs` / `mod.rs` files.
///
/// Candidates are returned in preference order (most specific first), so the
/// caller emits an edge for the first one that exists in `file_index`. External
/// crates (`std::...`, third-party) resolve to candidate paths that do not
/// exist and are thereby skipped silently by the caller, per the stage-4
/// "unresolved = skip silently" contract.
///
/// Prefix handling (each segment is a Rust module component, not a directory):
/// - `crate::X::Y` → target module path is `[X, Y]` rooted at the crate's `src/`.
/// - `self::X::Y` → target is current-module components + `[X, Y]`.
/// - `super::...` → one component is dropped from the current module path per
///   `super::`; the remainder is appended.
///
/// Current-module components are derived from the importing file's path:
/// - `src/foo/a.rs` → `[foo, a]` (a leaf module).
/// - `src/foo/mod.rs` → `[foo]` (the directory IS the module).
fn resolve_rust_use(module_ref: &str, importing_file: &str, ctx: &ResolverContext) -> Vec<String> {
    let importing_abs = ctx.repo_root.join(importing_file);
    let Some(parent) = importing_abs.parent() else {
        return Vec::new();
    };
    let Some(Some(crate_src_rel)) = ctx.rust_crate_src_by_dir.get(parent).cloned() else {
        return Vec::new();
    };

    let mut segments: Vec<&str> = module_ref.split("::").collect();

    // Compute the module path of the target, rooted as segments relative to
    // the crate `src/` directory. `target_components` is a list of module
    // names; we'll map it onto `.rs` / `mod.rs` candidates at the end.
    let target_components: Vec<String> = match segments.first().copied() {
        Some("crate") => {
            segments.remove(0);
            segments.iter().map(|s| s.to_string()).collect()
        }
        Some("self") => {
            segments.remove(0);
            let mut current = rust_current_module_components(importing_file, &crate_src_rel);
            current.extend(segments.iter().map(|s| s.to_string()));
            current
        }
        Some("super") => {
            let mut current = rust_current_module_components(importing_file, &crate_src_rel);
            while segments.first().copied() == Some("super") {
                segments.remove(0);
                if current.pop().is_none() {
                    return Vec::new(); // walked above the crate root
                }
            }
            current.extend(segments.iter().map(|s| s.to_string()));
            current
        }
        Some(first) => {
            // Bare path: treat as crate-relative only when the first segment
            // matches a top-level directory or file under the crate `src/`.
            if !rust_crate_has_top_level(ctx, &crate_src_rel, first) {
                return Vec::new();
            }
            segments.iter().map(|s| s.to_string()).collect()
        }
        None => return Vec::new(),
    };

    if target_components.is_empty() {
        // `use crate;` / `use self;` / `use super;` — no concrete target.
        return Vec::new();
    }

    let mut full: Vec<String> = crate_src_rel;
    full.extend(target_components);

    // Produce candidates in preference order. Full path first (longest match),
    // then the sub-item fallback that drops the last segment.
    let mut candidates: Vec<String> = Vec::new();
    push_rust_candidates(&full, &mut candidates);
    if full.len() > 1 {
        let trimmed = &full[..full.len() - 1];
        push_rust_candidates(trimmed, &mut candidates);
    }
    candidates
}

/// Compute the current module path components from an importing file path.
///
/// Strips the crate `src/` prefix, then:
/// - for `mod.rs`, uses the enclosing directory's trailing name(s);
/// - for a leaf `.rs`, appends the file stem.
///
/// Returns an empty vector for the crate root (`src/lib.rs`, `src/main.rs`).
fn rust_current_module_components(importing_file: &str, crate_src_rel: &[String]) -> Vec<String> {
    let crate_src_prefix = crate_src_rel.join("/");
    let prefix_with_slash = if crate_src_prefix.is_empty() {
        String::new()
    } else {
        format!("{crate_src_prefix}/")
    };
    let rest = importing_file
        .strip_prefix(&prefix_with_slash)
        .unwrap_or(importing_file);
    let path = Path::new(rest);
    let parent = path.parent().and_then(|p| p.to_str()).unwrap_or("");
    let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");

    let mut components: Vec<String> = if parent.is_empty() {
        Vec::new()
    } else {
        parent.split('/').map(String::from).collect()
    };

    // Crate-root files (`lib.rs`, `main.rs`) ARE the crate module; they add
    // no component. `mod.rs` IS its parent directory's module; it adds no
    // component either. Any other stem names a leaf submodule.
    let is_crate_root = components.is_empty() && matches!(file_stem, "lib" | "main");
    let is_mod_rs = file_stem == "mod";
    if !is_crate_root && !is_mod_rs && !file_stem.is_empty() {
        components.push(file_stem.to_string());
    }
    components
}

/// Push `<base>.rs` and `<base>/mod.rs` onto `out`, joined with `/`.
fn push_rust_candidates(base: &[String], out: &mut Vec<String>) {
    if base.is_empty() {
        return;
    }
    let joined = base.join("/");
    out.push(format!("{joined}.rs"));
    out.push(format!("{joined}/mod.rs"));
}

/// Walk up from `parent` (absolute) to find the `Cargo.toml`-owning crate and
/// return its `src/` directory as repo-relative path segments.
///
/// Returns `None` when no `Cargo.toml` is found or when the candidate crate
/// root has no `src/` directory (e.g., workspace roots).
fn rust_crate_src_walk(repo_root: &Path, parent: &Path) -> Option<Vec<String>> {
    let mut cursor: Option<&Path> = Some(parent);

    while let Some(dir) = cursor {
        if dir.join("Cargo.toml").is_file() {
            let src_dir = dir.join("src");
            if !src_dir.is_dir() {
                return None;
            }
            let rel = src_dir.strip_prefix(repo_root).ok()?;
            let segments: Vec<String> = rel
                .components()
                .filter_map(|c| c.as_os_str().to_str().map(String::from))
                .collect();
            return Some(segments);
        }
        if dir == repo_root {
            // Last chance at the repo root itself.
            if repo_root.join("Cargo.toml").is_file() && repo_root.join("src").is_dir() {
                return Some(vec!["src".to_string()]);
            }
            return None;
        }
        cursor = dir.parent();
        // Stop if we've walked out of the repo.
        if let Some(next) = cursor {
            if !next.starts_with(repo_root) && next != repo_root {
                return None;
            }
        }
    }
    None
}

/// True if `first_segment` names a top-level `.rs` file or directory under
/// the crate `src/`, as observed in the graph's `file_index`.
///
/// Uses `ctx.file_index` / `ctx.files_by_dir` instead of a filesystem probe so
/// redacted and `.gitignore`d files do not falsely claim to exist (the graph
/// never indexed them, so stage 4 would drop the edge anyway).
fn rust_crate_has_top_level(
    ctx: &ResolverContext,
    crate_src_rel: &[String],
    first_segment: &str,
) -> bool {
    let crate_src_prefix = crate_src_rel.join("/");
    let rs_key = if crate_src_prefix.is_empty() {
        format!("{first_segment}.rs")
    } else {
        format!("{crate_src_prefix}/{first_segment}.rs")
    };
    if ctx.file_index.contains_key(&rs_key) {
        return true;
    }
    let dir_key = if crate_src_prefix.is_empty() {
        first_segment.to_string()
    } else {
        format!("{crate_src_prefix}/{first_segment}")
    };
    ctx.files_by_dir.contains_key(&dir_key)
}

/// Resolve a Go import string to every `.go` file in the target package.
///
/// Returns empty when the repo has no `go.mod` or the import does not begin
/// with the declared module prefix. Otherwise strips the prefix and returns
/// every `.go` file the graph indexed inside the remainder directory (sub-
/// packages are separate import targets).
fn resolve_go_import(module_ref: &str, ctx: &ResolverContext) -> Vec<String> {
    let Some(prefix) = ctx.go_module_prefix.as_deref() else {
        return Vec::new();
    };

    // Match either `prefix/...` or exactly `prefix`.
    let remainder = if module_ref == prefix {
        ""
    } else {
        match ctx.go_module_prefix_slash.as_deref() {
            Some(slash_pfx) => match module_ref.strip_prefix(slash_pfx) {
                Some(rest) => rest,
                None => return Vec::new(),
            },
            None => return Vec::new(),
        }
    };

    let Some(files) = ctx.files_by_dir.get(remainder) else {
        return Vec::new();
    };

    let mut candidates = Vec::new();
    for name in files {
        if !name.ends_with(".go") {
            continue;
        }
        let rel = if remainder.is_empty() {
            name.clone()
        } else {
            format!("{remainder}/{name}")
        };
        candidates.push(rel);
    }
    candidates
}

/// Resolve `..` and `.` components in `path` without touching the filesystem.
fn normalize_path(path: &Path) -> PathBuf {
    let mut parts: Vec<Component> = Vec::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                parts.pop();
            }
            Component::CurDir => {}
            other => parts.push(other),
        }
    }
    parts.iter().collect()
}
