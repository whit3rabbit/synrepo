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
//! - Import paths are resolved for TypeScript (relative `./` and `../` paths)
//!   and Python (dotted-name to slash-path). Rust `use` last-names are skipped.
//! - Unresolved names are silently skipped — no error, no placeholder edge.
//! - Cross-file edges from unchanged files are preserved from the previous cycle
//!   because the delete cascade on changed file nodes cleans up stale edges.

use std::{
    collections::HashMap,
    path::{Component, Path, PathBuf},
};

use crate::{
    core::ids::{FileNodeId, NodeId, SymbolNodeId},
    pipeline::structural::ids::derive_edge_id,
    pipeline::structural::provenance::make_provenance,
    structure::{
        graph::{Edge, EdgeKind, Epistemic, GraphStore},
        parse::{ExtractedCallRef, ExtractedImportRef},
    },
};

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
        name_index
            .entry(short.to_string())
            .or_default()
            .push(*sym_id);
    }

    // Build file path index from all files currently in the graph.
    let all_files = graph.all_file_paths()?;
    let file_index: HashMap<String, FileNodeId> = all_files.into_iter().collect();

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

        // Imports edges: file → imported file
        for import_ref in &item.import_refs {
            let candidates = resolve_import_ref(&import_ref.module_ref, &item.file_path);
            let target_id = candidates
                .into_iter()
                .find_map(|p| file_index.get(&p).copied());

            if let Some(target_id) = target_id {
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

/// Attempt to resolve a module reference to a repo-relative file path.
///
/// Returns all potential candidate paths since the actual extension is missing.
/// Handles TypeScript/JavaScript relative imports and Python dotted imports.
fn resolve_import_ref(module_ref: &str, importing_file: &str) -> Vec<String> {
    if module_ref.is_empty() {
        return vec![];
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
