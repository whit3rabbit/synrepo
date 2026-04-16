use crate::pipeline::repair::{
    DriftClass, RepairAction, RepairFinding, RepairSurface, Severity,
};

use super::{RepairContext, SurfaceCheck};

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
