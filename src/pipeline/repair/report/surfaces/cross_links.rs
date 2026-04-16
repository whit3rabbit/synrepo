use crate::pipeline::repair::{
    DriftClass, RepairAction, RepairFinding, RepairSurface, Severity,
};

use super::{RepairContext, SurfaceCheck};

pub struct ProposedLinksOverlayCheck;

impl SurfaceCheck for ProposedLinksOverlayCheck {
    fn surface(&self) -> RepairSurface {
        RepairSurface::ProposedLinksOverlay
    }

    fn evaluate(&self, ctx: &RepairContext) -> Vec<RepairFinding> {
        use crate::store::overlay::SqliteOverlayStore;

        let overlay_dir = ctx.synrepo_dir.join("overlay");
        let overlay_db = SqliteOverlayStore::db_path(&overlay_dir);
        if !overlay_db.exists() {
            return vec![absent_cross_link_finding(
                "Cross-link overlay has not been materialized yet (no overlay.db).",
            )];
        }

        match scan_cross_links(ctx.synrepo_dir) {
            Ok(scan) => {
                if scan.total == 0 {
                    return vec![absent_cross_link_finding(
                        "Cross-link overlay is empty; no candidates to evaluate.",
                    )];
                }
                let mut out = Vec::new();
                for row in scan.drifted {
                    out.push(drifted_cross_link_finding(row));
                }
                if out.is_empty() {
                    out.push(RepairFinding {
                        surface: self.surface(),
                        drift_class: DriftClass::Current,
                        severity: Severity::Actionable,
                        target_id: None,
                        recommended_action: RepairAction::None,
                        notes: Some(format!(
                            "{} cross-link candidates are current ({} pending promotion).",
                            scan.total, scan.pending_promotion_count
                        )),
                    });
                } else {
                    // Add a summary note with state breakdown at the end when there are drift findings.
                    out.push(RepairFinding {
                        surface: self.surface(),
                        drift_class: DriftClass::Current,
                        severity: Severity::ReportOnly,
                        target_id: None,
                        recommended_action: RepairAction::None,
                        notes: Some(format!(
                            "State breakdown: {} total ({} active, {} pending promotion, {} promoted, {} rejected).",
                            scan.total, scan.active_count, scan.pending_promotion_count, scan.promoted_count, scan.rejected_count
                        )),
                    });
                }
                out
            }
            Err(err) => vec![RepairFinding {
                surface: self.surface(),
                drift_class: DriftClass::Blocked,
                severity: Severity::Blocked,
                target_id: None,
                recommended_action: RepairAction::ManualReview,
                notes: Some(format!("Cannot evaluate cross-link staleness: {err}")),
            }],
        }
    }
}

struct CrossLinkScan {
    total: usize,
    active_count: usize,
    pending_promotion_count: usize,
    promoted_count: usize,
    rejected_count: usize,
    drifted: Vec<DriftedCrossLink>,
}

enum CrossLinkDrift {
    Stale,
    SourceDeleted,
    PendingPromotion,
}

struct DriftedCrossLink {
    from_node: String,
    to_node: String,
    kind: String,
    classification: CrossLinkDrift,
}

fn absent_cross_link_finding(note: &str) -> RepairFinding {
    RepairFinding {
        surface: RepairSurface::ProposedLinksOverlay,
        drift_class: DriftClass::Absent,
        severity: Severity::ReportOnly,
        target_id: None,
        recommended_action: RepairAction::None,
        notes: Some(note.to_string()),
    }
}

fn drifted_cross_link_finding(row: DriftedCrossLink) -> RepairFinding {
    let target = format!(
        "from={} to={} kind={}",
        row.from_node, row.to_node, row.kind
    );
    match row.classification {
        CrossLinkDrift::Stale => RepairFinding {
            surface: RepairSurface::ProposedLinksOverlay,
            drift_class: DriftClass::Stale,
            severity: Severity::Actionable,
            target_id: Some(target),
            recommended_action: RepairAction::RevalidateLinks,
            notes: Some(
                "Stored endpoint hash no longer matches the current graph content.".to_string(),
            ),
        },
        CrossLinkDrift::SourceDeleted => RepairFinding {
            surface: RepairSurface::ProposedLinksOverlay,
            drift_class: DriftClass::SourceDeleted,
            severity: Severity::ReportOnly,
            target_id: Some(target),
            recommended_action: RepairAction::ManualReview,
            notes: Some("One or both endpoints are absent from the current graph.".to_string()),
        },
        CrossLinkDrift::PendingPromotion => RepairFinding {
            surface: RepairSurface::ProposedLinksOverlay,
            drift_class: DriftClass::Stale,
            severity: Severity::Actionable,
            target_id: Some(target),
            recommended_action: RepairAction::ManualReview,
            notes: Some(
                "Candidate stuck in pending_promotion after crash; run synrepo sync to resolve."
                    .to_string(),
            ),
        },
    }
}

fn scan_cross_links(synrepo_dir: &std::path::Path) -> crate::Result<CrossLinkScan> {
    use crate::core::ids::NodeId;
    use crate::store::overlay::SqliteOverlayStore;
    use crate::store::sqlite::SqliteGraphStore;
    use std::str::FromStr;

    let overlay = SqliteOverlayStore::open_existing(&synrepo_dir.join("overlay"))?;

    // Get state counts from the overlay.
    let state_counts = overlay.cross_link_state_counts()?;
    let pending_promotion_count = state_counts.pending_promotion;

    // Surface pending_promotion rows as a distinct drift class.
    let pending = overlay.pending_promotion_rows()?;
    let mut drifted: Vec<DriftedCrossLink> = pending
        .into_iter()
        .map(|row| DriftedCrossLink {
            from_node: row.from_node,
            to_node: row.to_node,
            kind: row.kind,
            classification: CrossLinkDrift::PendingPromotion,
        })
        .collect();

    let rows = overlay.cross_link_hashes()?;
    if rows.is_empty() && drifted.is_empty() {
        return Ok(CrossLinkScan {
            total: 0,
            active_count: state_counts.active,
            pending_promotion_count,
            promoted_count: state_counts.promoted,
            rejected_count: state_counts.rejected,
            drifted: Vec::new(),
        });
    }

    let graph = SqliteGraphStore::open_existing(&synrepo_dir.join("graph"))?;

    let total = rows.len();
    for row in rows {
        let from = NodeId::from_str(&row.from_node);
        let to = NodeId::from_str(&row.to_node);
        let (Ok(from), Ok(to)) = (from, to) else {
            drifted.push(DriftedCrossLink {
                from_node: row.from_node,
                to_node: row.to_node,
                kind: row.kind,
                classification: CrossLinkDrift::SourceDeleted,
            });
            continue;
        };

        let current_from = current_endpoint_hash(&graph, from)?;
        let current_to = current_endpoint_hash(&graph, to)?;

        let classification = match (current_from, current_to) {
            (Some(f), Some(t)) if f == row.from_content_hash && t == row.to_content_hash => {
                continue;
            }
            (Some(_), Some(_)) => CrossLinkDrift::Stale,
            _ => CrossLinkDrift::SourceDeleted,
        };
        drifted.push(DriftedCrossLink {
            from_node: row.from_node,
            to_node: row.to_node,
            kind: row.kind,
            classification,
        });
    }

    Ok(CrossLinkScan {
        total,
        active_count: state_counts.active,
        pending_promotion_count,
        promoted_count: state_counts.promoted,
        rejected_count: state_counts.rejected,
        drifted,
    })
}

fn current_endpoint_hash(
    graph: &crate::store::sqlite::SqliteGraphStore,
    node: crate::core::ids::NodeId,
) -> crate::Result<Option<String>> {
    use crate::core::ids::NodeId;
    use crate::structure::graph::GraphStore;

    match node {
        NodeId::File(file_id) => Ok(graph.get_file(file_id)?.map(|f| f.content_hash)),
        NodeId::Symbol(sym_id) => {
            let Some(sym) = graph.get_symbol(sym_id)? else {
                return Ok(None);
            };
            Ok(graph.get_file(sym.file_id)?.map(|f| f.content_hash))
        }
        NodeId::Concept(concept_id) => {
            let Some(concept) = graph.get_concept(concept_id)? else {
                return Ok(None);
            };
            Ok(graph.file_by_path(&concept.path)?.map(|f| f.content_hash))
        }
    }
}
