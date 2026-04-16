use crate::pipeline::diagnostics::{ReconcileHealth, WriterStatus};
use crate::pipeline::export::load_manifest;
use crate::pipeline::repair::{
    declared_links::check_declared_links, DriftClass, RepairAction, RepairFinding, RepairSurface,
    Severity,
};

use super::{RepairContext, SurfaceCheck};

pub struct WriterLockCheck;

impl SurfaceCheck for WriterLockCheck {
    fn surface(&self) -> RepairSurface {
        RepairSurface::WriterLock
    }

    fn evaluate(&self, ctx: &RepairContext) -> Vec<RepairFinding> {
        let finding = match &ctx.diagnostics.writer_status {
            WriterStatus::HeldByOther { pid } => RepairFinding {
                surface: self.surface(),
                drift_class: DriftClass::Blocked,
                severity: Severity::Blocked,
                target_id: Some(pid.to_string()),
                recommended_action: RepairAction::ManualReview,
                notes: Some(format!(
                    "Writer lock held by pid {pid}. Verify the process is alive before removing the lock."
                )),
            },
            WriterStatus::Free | WriterStatus::HeldBySelf => RepairFinding {
                surface: self.surface(),
                drift_class: DriftClass::Current,
                severity: Severity::Actionable,
                target_id: None,
                recommended_action: RepairAction::None,
                notes: None,
            },
        };
        vec![finding]
    }
}

pub struct StoreMaintenanceCheck;

impl SurfaceCheck for StoreMaintenanceCheck {
    fn surface(&self) -> RepairSurface {
        RepairSurface::StoreMaintenance
    }

    fn evaluate(&self, ctx: &RepairContext) -> Vec<RepairFinding> {
        let finding = match ctx.maint_plan {
            Ok(plan) if plan.has_work() => {
                let store_names: Vec<String> = plan
                    .pending_actions()
                    .map(|a| a.store_id.as_str().to_string())
                    .collect();
                RepairFinding {
                    surface: self.surface(),
                    drift_class: DriftClass::Stale,
                    severity: Severity::Actionable,
                    target_id: None,
                    recommended_action: RepairAction::RunMaintenance,
                    notes: Some(format!(
                        "Stores needing maintenance: {}",
                        store_names.join(", ")
                    )),
                }
            }
            Ok(_) => RepairFinding {
                surface: self.surface(),
                drift_class: DriftClass::Current,
                severity: Severity::Actionable,
                target_id: None,
                recommended_action: RepairAction::None,
                notes: None,
            },
            Err(err) => RepairFinding {
                surface: self.surface(),
                drift_class: DriftClass::Blocked,
                severity: Severity::Blocked,
                target_id: None,
                recommended_action: RepairAction::ManualReview,
                notes: Some(format!("Cannot evaluate storage: {err}")),
            },
        };
        vec![finding]
    }
}

pub struct StructuralRefreshCheck;

impl SurfaceCheck for StructuralRefreshCheck {
    fn surface(&self) -> RepairSurface {
        RepairSurface::StructuralRefresh
    }

    fn evaluate(&self, ctx: &RepairContext) -> Vec<RepairFinding> {
        let (drift, severity, action, notes) = match &ctx.diagnostics.reconcile_health {
            ReconcileHealth::Current => (
                DriftClass::Current,
                Severity::Actionable,
                RepairAction::None,
                None,
            ),
            ReconcileHealth::Unknown => (
                DriftClass::Absent,
                Severity::Actionable,
                RepairAction::RunReconcile,
                Some(
                    "Graph has never been populated. Run `synrepo reconcile` or `synrepo init`."
                        .to_string(),
                ),
            ),
            ReconcileHealth::Stale { last_outcome } => (
                DriftClass::Stale,
                Severity::Actionable,
                RepairAction::RunReconcile,
                Some(format!("Last reconcile outcome: {last_outcome}")),
            ),
        };

        vec![RepairFinding {
            surface: self.surface(),
            drift_class: drift,
            severity,
            target_id: None,
            recommended_action: action,
            notes,
        }]
    }
}

pub struct DeclaredLinksCheck;

impl SurfaceCheck for DeclaredLinksCheck {
    fn surface(&self) -> RepairSurface {
        RepairSurface::DeclaredLinks
    }

    fn evaluate(&self, ctx: &RepairContext) -> Vec<RepairFinding> {
        vec![check_declared_links(ctx.synrepo_dir)]
    }
}

pub struct UnsupportedSurfaceCheck;

impl SurfaceCheck for UnsupportedSurfaceCheck {
    fn surface(&self) -> RepairSurface {
        RepairSurface::StaleRationale
    }

    fn evaluate(&self, _ctx: &RepairContext) -> Vec<RepairFinding> {
        vec![RepairFinding {
            surface: self.surface(),
            drift_class: DriftClass::Unsupported,
            severity: Severity::Unsupported,
            target_id: None,
            recommended_action: RepairAction::NotSupported,
            notes: Some("Rationale drift scoring is not yet implemented.".to_string()),
        }]
    }
}

pub struct ExportSurfaceCheck;

impl SurfaceCheck for ExportSurfaceCheck {
    fn surface(&self) -> RepairSurface {
        RepairSurface::ExportSurface
    }

    fn evaluate(&self, ctx: &RepairContext) -> Vec<RepairFinding> {
        let manifest = load_manifest(ctx.repo_root, ctx.config);

        match manifest {
            None => vec![RepairFinding {
                surface: self.surface(),
                drift_class: DriftClass::Absent,
                severity: Severity::ReportOnly,
                target_id: None,
                recommended_action: RepairAction::None,
                notes: Some(
                    "Export directory has not been generated yet. Run `synrepo export`.".to_string(),
                ),
            }],
            Some(manifest) => {
                let current_epoch = ctx
                    .diagnostics
                    .last_reconcile
                    .as_ref()
                    .map(|r| r.last_reconcile_at.as_str())
                    .unwrap_or_default();
                if manifest.last_reconcile_at == current_epoch {
                    vec![RepairFinding {
                        surface: self.surface(),
                        drift_class: DriftClass::Current,
                        severity: Severity::Actionable,
                        target_id: None,
                        recommended_action: RepairAction::None,
                        notes: None,
                    }]
                } else {
                    vec![RepairFinding {
                        surface: self.surface(),
                        drift_class: DriftClass::Stale,
                        severity: Severity::Actionable,
                        target_id: None,
                        recommended_action: RepairAction::RegenerateExports,
                        notes: Some(format!(
                            "Export was generated at reconcile epoch `{}`, but current epoch is `{}`.",
                            manifest.last_reconcile_at, current_epoch
                        )),
                    }]
                }
            }
        }
    }
}

pub struct CommentaryOverlayCheck;

impl SurfaceCheck for CommentaryOverlayCheck {
    fn surface(&self) -> RepairSurface {
        RepairSurface::CommentaryOverlayEntries
    }

    fn evaluate(&self, ctx: &RepairContext) -> Vec<RepairFinding> {
        use crate::store::overlay::SqliteOverlayStore;

        let overlay_dir = ctx.synrepo_dir.join("overlay");
        let overlay_db = SqliteOverlayStore::db_path(&overlay_dir);
        if !overlay_db.exists() {
            return vec![RepairFinding {
                surface: self.surface(),
                drift_class: DriftClass::Absent,
                severity: Severity::ReportOnly,
                target_id: None,
                recommended_action: RepairAction::None,
                notes: Some(
                    "Commentary overlay has not been materialized yet (no overlay.db).".to_string(),
                ),
            }];
        }

        match scan_commentary_staleness(ctx.synrepo_dir) {
            Ok(CommentaryScan { total, stale }) if stale > 0 => vec![RepairFinding {
                surface: self.surface(),
                drift_class: DriftClass::Stale,
                severity: Severity::Actionable,
                target_id: None,
                recommended_action: RepairAction::RefreshCommentary,
                notes: Some(format!(
                    "{stale} of {total} commentary entries are stale against the current graph."
                )),
            }],
            Ok(CommentaryScan { total, .. }) => vec![RepairFinding {
                surface: self.surface(),
                drift_class: DriftClass::Current,
                severity: Severity::Actionable,
                target_id: None,
                recommended_action: RepairAction::None,
                notes: Some(format!("{total} commentary entries are current.")),
            }],
            Err(err) => vec![RepairFinding {
                surface: self.surface(),
                drift_class: DriftClass::Blocked,
                severity: Severity::Blocked,
                target_id: None,
                recommended_action: RepairAction::ManualReview,
                notes: Some(format!("Cannot evaluate commentary staleness: {err}")),
            }],
        }
    }
}

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
                        notes: Some(format!("{} cross-link candidates are current.", scan.total)),
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

pub struct EdgeDriftCheck;

impl SurfaceCheck for EdgeDriftCheck {
    fn surface(&self) -> RepairSurface {
        RepairSurface::EdgeDrift
    }

    fn evaluate(&self, ctx: &RepairContext) -> Vec<RepairFinding> {
        match edge_drift_findings(ctx.synrepo_dir) {
            Ok(findings) => findings,
            Err(err) => vec![RepairFinding {
                surface: self.surface(),
                drift_class: DriftClass::Blocked,
                severity: Severity::Blocked,
                target_id: None,
                recommended_action: RepairAction::ManualReview,
                notes: Some(format!("Cannot evaluate edge drift: {err}")),
            }],
        }
    }
}

pub struct RetiredObservationsCheck;

impl SurfaceCheck for RetiredObservationsCheck {
    fn surface(&self) -> RepairSurface {
        RepairSurface::RetiredObservations
    }

    fn evaluate(&self, ctx: &RepairContext) -> Vec<RepairFinding> {
        match retired_observations_findings(ctx.synrepo_dir) {
            Ok(findings) => findings,
            Err(err) => vec![RepairFinding {
                surface: self.surface(),
                drift_class: DriftClass::Blocked,
                severity: Severity::Blocked,
                target_id: None,
                recommended_action: RepairAction::ManualReview,
                notes: Some(format!("Cannot evaluate retired observations: {err}")),
            }],
        }
    }
}

// Helper functions and internal logic migrated from report.rs

struct CommentaryScan {
    total: usize,
    stale: usize,
}

fn scan_commentary_staleness(synrepo_dir: &std::path::Path) -> crate::Result<CommentaryScan> {
    use crate::core::ids::NodeId;
    use crate::pipeline::repair::commentary::resolve_commentary_node;
    use crate::store::overlay::SqliteOverlayStore;
    use crate::store::sqlite::SqliteGraphStore;
    use std::str::FromStr;

    let overlay = SqliteOverlayStore::open_existing(&synrepo_dir.join("overlay"))?;
    let rows = overlay.commentary_hashes()?;

    let graph = SqliteGraphStore::open_existing(&synrepo_dir.join("graph"))?;

    let mut total = 0usize;
    let mut stale = 0usize;
    for (node_id_str, stored_hash) in rows {
        total += 1;
        let Ok(node_id) = NodeId::from_str(&node_id_str) else {
            stale += 1;
            continue;
        };
        let fresh = resolve_commentary_node(&graph, node_id)?
            .is_some_and(|snap| snap.content_hash == stored_hash);
        if !fresh {
            stale += 1;
        }
    }

    Ok(CommentaryScan { total, stale })
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
    }
}

enum CrossLinkDrift {
    Stale,
    SourceDeleted,
}

struct DriftedCrossLink {
    from_node: String,
    to_node: String,
    kind: String,
    classification: CrossLinkDrift,
}

struct CrossLinkScan {
    total: usize,
    drifted: Vec<DriftedCrossLink>,
}

fn scan_cross_links(synrepo_dir: &std::path::Path) -> crate::Result<CrossLinkScan> {
    use crate::core::ids::NodeId;
    use crate::store::overlay::SqliteOverlayStore;
    use crate::store::sqlite::SqliteGraphStore;
    use std::str::FromStr;

    let overlay = SqliteOverlayStore::open_existing(&synrepo_dir.join("overlay"))?;
    let rows = overlay.cross_link_hashes()?;
    if rows.is_empty() {
        return Ok(CrossLinkScan {
            total: 0,
            drifted: Vec::new(),
        });
    }

    let graph = SqliteGraphStore::open_existing(&synrepo_dir.join("graph"))?;

    let mut drifted = Vec::new();
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

    Ok(CrossLinkScan { total, drifted })
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

fn edge_drift_findings(synrepo_dir: &std::path::Path) -> crate::Result<Vec<RepairFinding>> {
    use crate::store::sqlite::SqliteGraphStore;
    use crate::structure::graph::GraphStore;

    let graph_dir = synrepo_dir.join("graph");
    let graph_db = graph_dir.join("nodes.db");

    if !graph_db.exists() {
        return Ok(vec![RepairFinding {
            surface: RepairSurface::EdgeDrift,
            drift_class: DriftClass::Absent,
            severity: Severity::Unsupported,
            target_id: None,
            recommended_action: RepairAction::None,
            notes: Some("graph store not materialized; edge drift check skipped".to_string()),
        }]);
    }

    let graph = SqliteGraphStore::open_existing(&graph_dir)?;

    let Some(revision) = graph.latest_drift_revision()? else {
        return Ok(vec![RepairFinding {
            surface: RepairSurface::EdgeDrift,
            drift_class: DriftClass::Absent,
            severity: Severity::Unsupported,
            target_id: None,
            recommended_action: RepairAction::None,
            notes: Some(
                "no drift assessment performed yet; run a structural compile first".to_string(),
            ),
        }]);
    };

    let scores = graph.read_drift_scores(&revision).unwrap_or_default();

    if scores.is_empty() {
        return Ok(vec![RepairFinding {
            surface: RepairSurface::EdgeDrift,
            drift_class: DriftClass::Current,
            severity: Severity::Actionable,
            target_id: None,
            recommended_action: RepairAction::None,
            notes: Some("no drifted edges detected".to_string()),
        }]);
    }

    let high_drift: Vec<_> = scores.iter().filter(|(_, score)| *score >= 0.7).collect();
    let dead_edges: Vec<_> = scores
        .iter()
        .filter(|(_, score)| (*score - 1.0).abs() < f32::EPSILON)
        .collect();

    let mut findings = Vec::new();

    if !high_drift.is_empty() {
        findings.push(RepairFinding {
            surface: RepairSurface::EdgeDrift,
            drift_class: DriftClass::HighDriftEdge,
            severity: if dead_edges.is_empty() {
                Severity::ReportOnly
            } else {
                Severity::Actionable
            },
            target_id: None,
            recommended_action: if dead_edges.is_empty() {
                RepairAction::ManualReview
            } else {
                RepairAction::RunReconcile
            },
            notes: Some(format!(
                "{} edges at drift >= 0.7 ({} at 1.0, prunable)",
                high_drift.len(),
                dead_edges.len(),
            )),
        });
    }

    Ok(findings)
}

fn retired_observations_findings(
    synrepo_dir: &std::path::Path,
) -> crate::Result<Vec<RepairFinding>> {
    use crate::store::sqlite::SqliteGraphStore;
    use crate::structure::graph::GraphStore;

    let graph_dir = synrepo_dir.join("graph");
    let graph_db = graph_dir.join("nodes.db");

    if !graph_db.exists() {
        return Ok(vec![RepairFinding {
            surface: RepairSurface::RetiredObservations,
            drift_class: DriftClass::Absent,
            severity: Severity::Unsupported,
            target_id: None,
            recommended_action: RepairAction::None,
            notes: Some(
                "graph store not materialized; retired observations check skipped".to_string(),
            ),
        }]);
    }

    let graph = SqliteGraphStore::open_existing(&graph_dir)?;

    let all_edges = graph.all_edges()?;
    let active_edges = graph.active_edges()?;
    let total_edges = all_edges.len();
    let active_edges_count = active_edges.len();
    let retired_edges = total_edges.saturating_sub(active_edges_count);

    if retired_edges == 0 {
        return Ok(vec![RepairFinding {
            surface: RepairSurface::RetiredObservations,
            drift_class: DriftClass::Current,
            severity: Severity::Actionable,
            target_id: None,
            recommended_action: RepairAction::None,
            notes: Some("no retired observations to compact".to_string()),
        }]);
    }

    Ok(vec![RepairFinding {
        surface: RepairSurface::RetiredObservations,
        drift_class: DriftClass::Stale,
        severity: Severity::Actionable,
        target_id: None,
        recommended_action: RepairAction::CompactRetired,
        notes: Some(format!(
            "{} retired edges detected; run `synrepo sync` to compact",
            retired_edges
        )),
    }])
}
