use std::{
    fs,
    path::{Component, Path, PathBuf},
};

use super::{
    context::{CrossFilePending, ImportsMap, ResolverContext},
    rust_paths::resolve_rust_use,
};
use crate::{
    core::ids::{FileNodeId, NodeId},
    pipeline::structural::ids::derive_edge_id,
    pipeline::structural::provenance::make_provenance,
    structure::{
        graph::{Edge, EdgeKind, Epistemic, GraphStore},
        parse::Language,
    },
};

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

pub(super) fn emit_imports_for_file(
    graph: &mut dyn GraphStore,
    ctx: &ResolverContext,
    item: &CrossFilePending,
    revision: &str,
    imports_map: &mut ImportsMap,
) -> crate::Result<usize> {
    let importing_lang = Path::new(&item.file_path)
        .extension()
        .and_then(|e| e.to_str())
        .and_then(Language::from_extension);
    let mut emitted = 0usize;

    for import_ref in &item.import_refs {
        let candidates =
            resolve_import_ref(&import_ref.module_ref, &item.file_path, &item.root_id, ctx);
        let targets: Vec<FileNodeId> = if importing_lang == Some(Language::Go) {
            candidates
                .into_iter()
                .filter_map(|p| ctx.file_index.get(&(item.root_id.clone(), p)).copied())
                .collect()
        } else {
            candidates
                .into_iter()
                .find_map(|p| ctx.file_index.get(&(item.root_id.clone(), p)).copied())
                .into_iter()
                .collect()
        };
        for target_id in targets {
            if target_id == item.file_id {
                continue;
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
                owner_file_id: Some(item.file_id),
                last_observed_rev: None,
                retired_at_rev: None,
                epistemic: Epistemic::ParserObserved,
                provenance: make_provenance("stage4_imports", revision, &item.file_path, ""),
            };
            graph.insert_edge(edge)?;
            emitted += 1;

            imports_map
                .entry(item.file_id)
                .or_default()
                .insert(target_id);
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
/// - Python dotted imports (`foo.bar` -> `foo/bar.py`).
/// - Go imports whose prefix matches the local `go.mod` module declaration,
///   fanned out across every `.go` file in the resolved package directory.
pub(super) fn resolve_import_ref(
    module_ref: &str,
    importing_file: &str,
    root_id: &str,
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
        Some(Language::Rust) => return resolve_rust_use(module_ref, importing_file, root_id, ctx),
        Some(Language::Go) => {
            // `interpreted_string_literal` captures include the surrounding quotes.
            let stripped = module_ref
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .unwrap_or(module_ref);
            return resolve_go_import(stripped, root_id, ctx);
        }
        Some(Language::Dart) => return resolve_dart_import(module_ref, importing_file, ctx),
        _ => {}
    }

    // TypeScript / JavaScript relative imports: ./foo  ../bar/baz
    if module_ref.starts_with("./") || module_ref.starts_with("../") {
        let Some(dir) = Path::new(importing_file).parent() else {
            return vec![];
        };
        let joined = dir.join(module_ref);
        // `..` underflow returns None (escape outside the project root); drop
        // the import rather than emitting a candidate that could re-escape via
        // string concatenation downstream.
        let Some(normalized) = normalize_path(&joined) else {
            return vec![];
        };
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
            candidates.push(format!("{base}.{ext}"));
        }
        // Try index file inside the directory
        for ext in &["ts", "tsx", "js"] {
            candidates.push(format!("{base}/index.{ext}"));
        }
        return candidates;
    }

    // Python dotted import: foo.bar -> foo/bar.py
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

/// Resolve Dart `package:` and relative URI imports.
fn resolve_dart_import(
    module_ref: &str,
    importing_file: &str,
    ctx: &ResolverContext,
) -> Vec<String> {
    let module_ref = module_ref.trim_matches(['"', '\'']);
    if module_ref.is_empty() || module_ref.starts_with("dart:") {
        return Vec::new();
    }

    if let Some(package_prefix) = ctx.dart_package_prefix.as_deref() {
        if let Some(rest) = module_ref.strip_prefix(package_prefix) {
            return dart_file_candidates(&format!("lib/{rest}"));
        }
    }

    if module_ref.contains(':') {
        return Vec::new();
    }

    let Some(dir) = Path::new(importing_file).parent() else {
        return Vec::new();
    };
    let Some(normalized) = normalize_path(&dir.join(module_ref)) else {
        return Vec::new();
    };
    let Some(base) = normalized.to_str() else {
        return Vec::new();
    };
    dart_file_candidates(&base.replace('\\', "/"))
}

fn dart_file_candidates(base: &str) -> Vec<String> {
    if base.ends_with(".dart") {
        vec![base.to_string()]
    } else {
        vec![format!("{base}.dart")]
    }
}

/// Resolve a Go import string to every `.go` file in the target package.
///
/// Returns empty when the repo has no `go.mod` or the import does not begin
/// with the declared module prefix. Otherwise strips the prefix and returns
/// every `.go` file the graph indexed inside the remainder directory (sub-
/// packages are separate import targets).
fn resolve_go_import(module_ref: &str, root_id: &str, ctx: &ResolverContext) -> Vec<String> {
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

    let Some(files) = ctx
        .files_by_dir
        .get(&(root_id.to_string(), remainder.to_string()))
    else {
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
///
/// Returns `None` when a `..` would pop above the root (i.e. underflow). The
/// caller treats `None` as "this import escapes the project root, drop it".
/// Previously we silently dropped underflowing `..` components and relied on a
/// downstream `candidate.contains("..")` check to catch the leftover marker;
/// returning `None` makes the escape signal type-level and removes the need
/// for that string-level paranoia.
fn normalize_path(path: &Path) -> Option<PathBuf> {
    let mut parts: Vec<Component> = Vec::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                parts.pop()?;
            }
            Component::CurDir => {}
            other => parts.push(other),
        }
    }
    Some(parts.iter().collect())
}
