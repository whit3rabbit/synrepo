//! Persist identity resolutions to the graph.

use super::IdentityResolution;
use crate::core::ids::NodeId;
use crate::structure::graph::{derive_edge_id, Edge, EdgeKind, Epistemic, GraphStore};

/// Persist identity resolutions to the graph by writing the appropriate edges
/// and updating path history. Returns the number of edges written.
pub fn persist_resolutions(
    resolutions: &[IdentityResolution],
    graph: &mut dyn GraphStore,
    revision: &str,
) -> crate::Result<usize> {
    let mut edges_written = 0;

    for resolution in resolutions {
        match resolution {
            IdentityResolution::Rename {
                preserved_id,
                new_path,
            } => {
                let Some(mut file) = graph.get_file(*preserved_id)? else {
                    continue;
                };
                if let Some(dup) = graph.file_by_root_path(&file.root_id, new_path)? {
                    if dup.id != *preserved_id {
                        graph.delete_node(NodeId::File(dup.id))?;
                    }
                }
                let old_path = file.path.clone();
                file.path_history.insert(0, old_path);
                file.path = new_path.clone();
                graph.upsert_file(file)?;
                tracing::debug!(
                    file_id = %preserved_id,
                    new_path = %new_path,
                    "identity: structural rename resolution"
                );
            }
            IdentityResolution::Split {
                primary: (old_id, primary_path),
                secondaries,
            } => {
                for sec_path in secondaries {
                    if let Some(sec_file) = graph.file_by_path(sec_path)? {
                        let edge = Edge {
                            id: derive_edge_id(
                                NodeId::File(sec_file.id),
                                NodeId::File(*old_id),
                                EdgeKind::SplitFrom,
                            ),
                            from: NodeId::File(sec_file.id),
                            to: NodeId::File(*old_id),
                            kind: EdgeKind::SplitFrom,
                            owner_file_id: None,
                            last_observed_rev: None,
                            retired_at_rev: None,
                            epistemic: Epistemic::ParserObserved,
                            provenance: crate::core::provenance::Provenance::structural(
                                "identity_split",
                                revision,
                                vec![crate::core::provenance::SourceRef {
                                    file_id: Some(*old_id),
                                    path: primary_path.clone(),
                                    content_hash: String::new(),
                                }],
                            ),
                        };
                        graph.insert_edge(edge)?;
                        edges_written += 1;
                    }
                }
            }
            IdentityResolution::Merge { new_path, old_ids } => {
                if let Some(new_file) = graph.file_by_path(new_path)? {
                    for old_id in old_ids {
                        let edge = Edge {
                            id: derive_edge_id(
                                NodeId::File(new_file.id),
                                NodeId::File(*old_id),
                                EdgeKind::MergedFrom,
                            ),
                            from: NodeId::File(new_file.id),
                            to: NodeId::File(*old_id),
                            kind: EdgeKind::MergedFrom,
                            owner_file_id: None,
                            last_observed_rev: None,
                            retired_at_rev: None,
                            epistemic: Epistemic::ParserObserved,
                            provenance: crate::core::provenance::Provenance::structural(
                                "identity_merge",
                                revision,
                                vec![crate::core::provenance::SourceRef {
                                    file_id: Some(*old_id),
                                    path: new_path.clone(),
                                    content_hash: String::new(),
                                }],
                            ),
                        };
                        graph.insert_edge(edge)?;
                        edges_written += 1;
                    }

                    // Append old file paths to the new file's path_history.
                    let mut updated = new_file.clone();
                    for old_id in old_ids {
                        if let Some(old_file) = graph.get_file(*old_id)? {
                            updated.path_history.push(old_file.path.clone());
                        }
                    }
                    if updated.path_history.len() != new_file.path_history.len() {
                        graph.upsert_file(updated)?;
                    }
                }
            }
            IdentityResolution::GitRename {
                preserved_id,
                new_path,
            } => {
                // The pipeline already inserted a new file node at new_path
                // before the identity cascade ran. Delete that duplicate so
                // the preserved node can take its place.
                if let Some(dup) = graph.file_by_path(new_path)? {
                    if dup.id != *preserved_id {
                        graph.delete_node(NodeId::File(dup.id))?;
                    }
                }
                // Preserve the old node ID, update its path, and append the
                // old path to path_history.
                if let Some(mut file) = graph.get_file(*preserved_id)? {
                    let old_path = file.path.clone();
                    file.path_history.insert(0, old_path);
                    file.path = new_path.clone();
                    graph.upsert_file(file)?;
                }
                tracing::debug!(
                    file_id = %preserved_id,
                    new_path = %new_path,
                    "identity: git rename resolution"
                );
            }
            IdentityResolution::Breakage { orphaned, reason } => {
                tracing::info!(
                    file_id = %orphaned,
                    reason = %reason,
                    "identity: breakage resolution (file deleted without match)"
                );
            }
        }
    }

    Ok(edges_written)
}
