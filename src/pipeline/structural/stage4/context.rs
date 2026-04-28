use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use super::{imports::load_go_module_prefix, rust_paths::rust_crate_src_walk};
use crate::{
    core::ids::{FileNodeId, SymbolNodeId},
    structure::{
        graph::{GraphStore, SymbolKind, Visibility},
        parse::{ExtractedCallRef, ExtractedImportRef},
    },
};

pub(super) type NameIndex = HashMap<String, Vec<SymbolNodeId>>;
pub(super) type SymbolMetaMap = HashMap<SymbolNodeId, SymbolMeta>;
pub(super) type CallerIndex = HashMap<(FileNodeId, String, String), SymbolNodeId>;

/// Scope map: for each file, the set of files it imports (direct imports only).
/// Built as Imports edges are emitted, before the Calls resolution loop.
pub(super) type ImportsMap = HashMap<FileNodeId, HashSet<FileNodeId>>;

/// Metadata for a symbol used in call-resolution scoring.
#[derive(Clone, Debug)]
pub(super) struct SymbolMeta {
    pub(super) file_id: FileNodeId,
    pub(super) root_id: String,
    pub(super) visibility: Visibility,
    pub(super) kind: SymbolKind,
    pub(super) qualified_name: String,
}

/// Per-compile resolver state threaded into every import reference.
///
/// Built once at the top of `run_cross_file_resolution` so stage 4 does not
/// re-read `go.mod`, re-walk `Cargo.toml`, or re-scan package directories per
/// import_ref.
pub(super) struct ResolverContext {
    pub(super) repo_root: PathBuf,
    /// Every file the graph knows about, keyed by `(root_id, repo-relative path)`.
    pub(super) file_index: HashMap<(String, String), FileNodeId>,
    /// Files grouped by parent directory. The empty string key holds
    /// repo-root files. Used for O(1) "directory exists" checks and Go
    /// package fan-out without a filesystem walk.
    pub(super) files_by_dir: HashMap<(String, String), Vec<String>>,
    /// `module ...` line from `<repo_root>/go.mod`, or `None`.
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

/// Pending cross-file resolution work for one file parsed this cycle.
pub struct CrossFilePending {
    pub file_id: FileNodeId,
    pub root_id: String,
    pub file_path: String,
    pub call_refs: Vec<ExtractedCallRef>,
    pub import_refs: Vec<ExtractedImportRef>,
}

pub(super) fn build_indices(
    graph: &mut dyn GraphStore,
    pending: &[CrossFilePending],
    repo_root: &Path,
) -> crate::Result<(ResolverContext, NameIndex, SymbolMetaMap, CallerIndex)> {
    // Build short-name index and per-symbol metadata in a single pass using
    // the bulk resolver query (one SELECT, visibility parsed from the blob).
    // SQLite read-your-own-writes lets us see stages 1-3's inserts inside the
    // caller's open transaction.
    let all_symbols = graph.all_symbols_for_resolution()?;
    let mut name_index: NameIndex = HashMap::with_capacity(all_symbols.len());
    let mut symbol_meta: SymbolMetaMap = HashMap::with_capacity(all_symbols.len());
    let mut caller_index: CallerIndex = HashMap::with_capacity(all_symbols.len());
    for (sym_id, file_id, qname, kind, visibility, body_hash) in all_symbols {
        let Some(file) = graph.get_file(file_id)? else {
            continue;
        };
        let short = qname.rsplit("::").next().unwrap_or(qname.as_str());
        name_index
            .entry(short.to_string())
            .or_default()
            .push(sym_id);
        symbol_meta.insert(
            sym_id,
            SymbolMeta {
                file_id,
                root_id: file.root_id,
                visibility,
                kind,
                qualified_name: qname.clone(),
            },
        );
        caller_index.insert((file_id, qname, body_hash), sym_id);
    }

    // Build file_index and files_by_dir in a single pass so both share the
    // same allocation and enumerate the same set.
    let all_files = graph.all_file_paths()?;
    let mut file_index: HashMap<(String, String), FileNodeId> =
        HashMap::with_capacity(all_files.len());
    let mut files_by_dir: HashMap<(String, String), Vec<String>> = HashMap::new();
    for (path, file_id) in all_files {
        let Some(file) = graph.get_file(file_id)? else {
            continue;
        };
        let root_id = file.root_id;
        match path.rsplit_once('/') {
            Some((dir, file)) => {
                files_by_dir
                    .entry((root_id.clone(), dir.to_string()))
                    .or_default()
                    .push(file.to_string());
            }
            None => {
                files_by_dir
                    .entry((root_id.clone(), String::new()))
                    .or_default()
                    .push(path.clone());
            }
        }
        file_index.insert((root_id, path), file_id);
    }

    let go_module_prefix = load_go_module_prefix(repo_root);
    let go_module_prefix_slash = go_module_prefix.as_deref().map(|p| format!("{p}/"));

    // Precompute Rust `rust_crate_src` per unique parent directory of pending
    // `.rs` files. `rust_crate_src` walks up the filesystem looking for
    // `Cargo.toml`, so deduplicating by parent dir turns O(files x depth)
    // syscalls into O(unique_dirs x depth).
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

    Ok((ctx, name_index, symbol_meta, caller_index))
}
