use std::path::Path;

use crate::{
    core::ids::NodeId,
    store::sqlite::SqliteGraphStore,
    structure::graph::{EdgeKind, GraphStore},
};

use super::{DriftClass, RepairAction, RepairFinding, RepairSurface, Severity};

/// Check declared links: verify every Governs edge target still exists.
///
/// Uses `all_concept_paths` to iterate concepts, then `outbound` + `get_*` to
/// validate each edge target. Returns `DriftClass::Stale` when any edge points
/// to a node that has been deleted. `synrepo sync` will repair by running
/// `reconcile`, which re-resolves governs paths against the current file set.
pub(super) fn check_declared_links(synrepo_dir: &Path) -> RepairFinding {
    let graph_dir = synrepo_dir.join("graph");

    let store = match SqliteGraphStore::open_existing(&graph_dir) {
        Ok(s) => s,
        Err(_) => {
            return RepairFinding {
                surface: RepairSurface::DeclaredLinks,
                drift_class: DriftClass::Absent,
                severity: Severity::Unsupported,
                target_id: None,
                recommended_action: RepairAction::NotSupported,
                notes: Some(
                    "Graph store is not materialized; cannot check declared links.".to_string(),
                ),
            };
        }
    };

    let stats = match store.persisted_stats() {
        Ok(s) => s,
        Err(err) => {
            return RepairFinding {
                surface: RepairSurface::DeclaredLinks,
                drift_class: DriftClass::Blocked,
                severity: Severity::Blocked,
                target_id: None,
                recommended_action: RepairAction::ManualReview,
                notes: Some(format!("Cannot read graph stats: {err}")),
            };
        }
    };

    if stats.concept_nodes == 0 {
        return RepairFinding {
            surface: RepairSurface::DeclaredLinks,
            drift_class: DriftClass::Current,
            severity: Severity::Actionable,
            target_id: None,
            recommended_action: RepairAction::None,
            notes: Some("No concept nodes in graph; no declared links to verify.".to_string()),
        };
    }

    let governs_count = stats
        .edge_counts_by_kind
        .get("governs")
        .copied()
        .unwrap_or(0);
    if governs_count == 0 {
        return RepairFinding {
            surface: RepairSurface::DeclaredLinks,
            drift_class: DriftClass::Current,
            severity: Severity::Actionable,
            target_id: None,
            recommended_action: RepairAction::None,
            notes: Some(format!(
                "{} concept node(s) present but no Governs edges declared yet.",
                stats.concept_nodes
            )),
        };
    }

    let concept_paths = match store.all_concept_paths() {
        Ok(v) => v,
        Err(err) => {
            return RepairFinding {
                surface: RepairSurface::DeclaredLinks,
                drift_class: DriftClass::Blocked,
                severity: Severity::Blocked,
                target_id: None,
                recommended_action: RepairAction::ManualReview,
                notes: Some(format!("Cannot iterate concept nodes: {err}")),
            };
        }
    };

    let mut dangling: Vec<String> = Vec::new();
    for (concept_path, concept_id) in &concept_paths {
        let edges = match store.outbound(NodeId::Concept(*concept_id), Some(EdgeKind::Governs)) {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!(concept = %concept_path, error = %e, "failed to query outbound edges; skipping");
                continue;
            }
        };
        for edge in edges {
            let exists = match edge.to {
                NodeId::File(fid) => store.get_file(fid).ok().flatten().is_some(),
                NodeId::Symbol(sid) => store.get_symbol(sid).ok().flatten().is_some(),
                NodeId::Concept(cid) => store.get_concept(cid).ok().flatten().is_some(),
            };
            if !exists {
                dangling.push(concept_path.clone());
                break;
            }
        }
    }

    if dangling.is_empty() {
        return RepairFinding {
            surface: RepairSurface::DeclaredLinks,
            drift_class: DriftClass::Current,
            severity: Severity::Actionable,
            target_id: None,
            recommended_action: RepairAction::None,
            notes: Some(format!(
                "{} concept node(s), {} Governs edge(s) verified; all targets exist.",
                stats.concept_nodes, governs_count
            )),
        };
    }

    let sample: Vec<_> = dangling.iter().take(5).cloned().collect();
    let overflow = dangling.len().saturating_sub(5);
    let mut note = format!(
        "{} concept doc(s) have dangling Governs targets: {}",
        dangling.len(),
        sample.join(", ")
    );
    if overflow > 0 {
        note.push_str(&format!(" (and {overflow} more)"));
    }
    note.push_str(". Run `synrepo reconcile` to re-resolve edges.");

    RepairFinding {
        surface: RepairSurface::DeclaredLinks,
        drift_class: DriftClass::Stale,
        severity: Severity::Actionable,
        target_id: None,
        recommended_action: RepairAction::RunReconcile,
        notes: Some(note),
    }
}
