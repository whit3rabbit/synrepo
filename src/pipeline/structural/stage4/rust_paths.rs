use std::path::Path;

use super::context::ResolverContext;

/// Resolve a Rust `use` path to candidate repo-relative `.rs` / `mod.rs` files.
///
/// Candidates are returned in preference order (most specific first), so the
/// caller emits an edge for the first one that exists in `file_index`. External
/// crates (`std::...`, third-party) resolve to candidate paths that do not
/// exist and are thereby skipped silently by the caller, per the stage-4
/// "unresolved = skip silently" contract.
///
/// Prefix handling (each segment is a Rust module component, not a directory):
/// - `crate::X::Y` -> target module path is `[X, Y]` rooted at the crate's `src/`.
/// - `self::X::Y` -> target is current-module components + `[X, Y]`.
/// - `super::...` -> one component is dropped from the current module path per
///   `super::`; the remainder is appended.
///
/// Current-module components are derived from the importing file's path:
/// - `src/foo/a.rs` -> `[foo, a]` (a leaf module).
/// - `src/foo/mod.rs` -> `[foo]` (the directory IS the module).
pub(super) fn resolve_rust_use(
    module_ref: &str,
    importing_file: &str,
    ctx: &ResolverContext,
) -> Vec<String> {
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
        // `use crate;` / `use self;` / `use super;` -- no concrete target.
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
pub(super) fn rust_crate_src_walk(repo_root: &Path, parent: &Path) -> Option<Vec<String>> {
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
