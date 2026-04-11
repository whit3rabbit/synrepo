//! Stage 4: cross-file edge resolution.
//!
//! Runs after stages 1–3 have committed all file and symbol nodes. Builds an
//! in-memory name index from the graph, then emits `Calls` and `Imports` edges
//! for newly parsed files.
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

    super::with_transaction(graph, |graph| {
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
                let resolved = resolve_import_ref(&import_ref.module_ref, &item.file_path);
                let target_id = resolved.as_deref().and_then(|p| file_index.get(p)).copied();

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
                        epistemic: Epistemic::ParserObserved,
                        drift_score: 0.0,
                        provenance: make_provenance(
                            "stage4_imports",
                            revision,
                            &item.file_path,
                            "",
                        ),
                    };
                    graph.insert_edge(edge)?;
                    emitted += 1;
                }
            }
        }

        Ok(emitted)
    })
}

/// Attempt to resolve a module reference to a repo-relative file path.
///
/// Returns `Some(path)` when the reference can be resolved to a known
/// convention; returns `None` for unresolvable references (skipped silently).
fn resolve_import_ref(module_ref: &str, importing_file: &str) -> Option<String> {
    if module_ref.is_empty() {
        return None;
    }

    // TypeScript / JavaScript relative imports: ./foo  ../bar/baz
    if module_ref.starts_with("./") || module_ref.starts_with("../") {
        let dir = Path::new(importing_file).parent()?;
        let joined = dir.join(module_ref);
        let normalized = normalize_path(&joined);
        let base = normalized.to_str()?;

        // Try bare path + common extensions
        for ext in &["ts", "tsx", "js", "jsx", "mts", "cts"] {
            let candidate = format!("{base}.{ext}");
            if !candidate.contains("..") {
                return Some(candidate);
            }
        }
        // Try index file inside the directory
        for ext in &["ts", "tsx", "js"] {
            let candidate = format!("{base}/index.{ext}");
            if !candidate.contains("..") {
                return Some(candidate);
            }
        }
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
        return Some(candidate);
    }

    None
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
