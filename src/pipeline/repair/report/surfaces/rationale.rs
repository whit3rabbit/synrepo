use std::collections::HashMap;
use std::path::Path;

use crate::core::ids::{EdgeId, NodeId};
use crate::pipeline::repair::{DriftClass, RepairAction, RepairFinding, RepairSurface, Severity};
use crate::structure::graph::{EdgeKind, GraphReader};

use super::{RepairContext, SurfaceCheck};

// Lower than `EdgeDriftCheck`'s 0.7 cutoff because governed targets warrant an
// earlier re-read signal on the human-authored rationale.
const RATIONALE_DRIFT_THRESHOLD: f32 = 0.5;

pub struct StaleRationaleCheck;

impl SurfaceCheck for StaleRationaleCheck {
    fn surface(&self) -> RepairSurface {
        RepairSurface::StaleRationale
    }

    fn evaluate(&self, ctx: &RepairContext) -> Vec<RepairFinding> {
        match stale_rationale_findings(ctx.synrepo_dir) {
            Ok(findings) => findings,
            Err(err) => vec![RepairFinding {
                surface: RepairSurface::StaleRationale,
                drift_class: DriftClass::Blocked,
                severity: Severity::Blocked,
                target_id: None,
                recommended_action: RepairAction::ManualReview,
                notes: Some(format!("Cannot evaluate stale rationale: {err}")),
            }],
        }
    }
}

fn stale_rationale_findings(synrepo_dir: &Path) -> crate::Result<Vec<RepairFinding>> {
    use crate::store::sqlite::SqliteGraphStore;

    let graph_dir = synrepo_dir.join("graph");

    // Pre-check distinguishes "graph not materialized" (→ Absent) from "graph
    // exists but unreadable" (→ Blocked) — `open_existing` collapses both.
    if !SqliteGraphStore::db_path(&graph_dir).exists() {
        return Ok(vec![absent_finding(
            "graph store not materialized; stale rationale check skipped",
        )]);
    }

    let graph = SqliteGraphStore::open_existing(&graph_dir)?;

    let Some(revision) = graph.latest_drift_revision()? else {
        return Ok(vec![absent_finding(
            "no drift assessment performed yet; run a structural compile first",
        )]);
    };

    let scores: HashMap<EdgeId, f32> = graph.read_drift_scores(&revision)?.into_iter().collect();

    let concept_paths = graph.all_concept_paths()?;
    if concept_paths.is_empty() {
        return Ok(vec![current_finding(
            "no concept nodes in graph; no rationale to check",
        )]);
    }

    let mut findings = Vec::new();
    for (concept_path, concept_id) in &concept_paths {
        let governs = graph.outbound(NodeId::Concept(*concept_id), Some(EdgeKind::Governs))?;
        if governs.is_empty() {
            continue;
        }
        let mut drifted = 0usize;
        let mut max_score = 0.0f32;
        for edge in &governs {
            if let Some(score) = scores.get(&edge.id) {
                if *score >= RATIONALE_DRIFT_THRESHOLD {
                    drifted += 1;
                    if *score > max_score {
                        max_score = *score;
                    }
                }
            }
        }
        if drifted > 0 {
            findings.push(RepairFinding {
                surface: RepairSurface::StaleRationale,
                drift_class: DriftClass::Stale,
                severity: Severity::ReportOnly,
                target_id: Some(concept_path.clone()),
                recommended_action: RepairAction::ManualReview,
                notes: Some(format!(
                    "{} of {} governed target(s) drifted (Jaccard >= {:.1}, max {:.2}); re-read and refresh the rationale",
                    drifted,
                    governs.len(),
                    RATIONALE_DRIFT_THRESHOLD,
                    max_score,
                )),
            });
        }
    }

    if findings.is_empty() {
        findings.push(current_finding(
            "all governed targets within Jaccard threshold",
        ));
    }

    Ok(findings)
}

fn absent_finding(msg: &str) -> RepairFinding {
    RepairFinding {
        surface: RepairSurface::StaleRationale,
        drift_class: DriftClass::Absent,
        severity: Severity::ReportOnly,
        target_id: None,
        recommended_action: RepairAction::None,
        notes: Some(msg.to_string()),
    }
}

fn current_finding(msg: &str) -> RepairFinding {
    RepairFinding {
        surface: RepairSurface::StaleRationale,
        drift_class: DriftClass::Current,
        severity: Severity::ReportOnly,
        target_id: None,
        recommended_action: RepairAction::None,
        notes: Some(msg.to_string()),
    }
}
