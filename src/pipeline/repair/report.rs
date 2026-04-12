use std::path::Path;

use crate::{
    config::Config,
    pipeline::maintenance::{plan_maintenance, MaintenancePlan},
};

use super::{
    declared_links::check_declared_links, DriftClass, RepairAction, RepairFinding, RepairReport,
    RepairSurface, Severity,
};
use crate::pipeline::{
    diagnostics::{collect_diagnostics, ReconcileHealth, WriterStatus},
    writer::now_rfc3339,
};

/// Build a repair report by composing existing diagnostics and maintenance
/// planning. This is the read-only `check` path: no state is mutated.
pub fn build_repair_report(synrepo_dir: &Path, config: &Config) -> RepairReport {
    let maint_plan = plan_maintenance(synrepo_dir, config);
    assemble_repair_report(synrepo_dir, config, &maint_plan)
}

/// Inner report builder. Accepts a pre-computed `maint_plan` so callers that
/// also need to execute maintenance (e.g. `execute_sync`) don't call
/// `plan_maintenance` twice.
pub(super) fn assemble_repair_report(
    synrepo_dir: &Path,
    config: &Config,
    maint_plan: &crate::Result<MaintenancePlan>,
) -> RepairReport {
    let now = now_rfc3339();
    let mut findings = Vec::new();
    let diag = collect_diagnostics(synrepo_dir, config);

    findings.push(writer_lock_finding(&diag.writer_status));
    findings.push(store_maintenance_finding(maint_plan));
    findings.push(structural_refresh_finding(&diag.reconcile_health));
    findings.push(check_declared_links(synrepo_dir));
    findings.push(commentary_overlay_finding(synrepo_dir));
    findings.extend(unsupported_surface_findings());

    RepairReport {
        checked_at: now,
        findings,
    }
}

fn writer_lock_finding(status: &WriterStatus) -> RepairFinding {
    match status {
        WriterStatus::HeldByOther { pid } => RepairFinding {
            surface: RepairSurface::WriterLock,
            drift_class: DriftClass::Blocked,
            severity: Severity::Blocked,
            target_id: Some(pid.to_string()),
            recommended_action: RepairAction::ManualReview,
            notes: Some(format!(
                "Writer lock held by pid {pid}. Verify the process is alive before removing the lock."
            )),
        },
        WriterStatus::Free | WriterStatus::HeldBySelf => RepairFinding {
            surface: RepairSurface::WriterLock,
            drift_class: DriftClass::Current,
            severity: Severity::Actionable,
            target_id: None,
            recommended_action: RepairAction::None,
            notes: None,
        },
    }
}

fn store_maintenance_finding(maint_plan: &crate::Result<MaintenancePlan>) -> RepairFinding {
    match maint_plan {
        Ok(plan) if plan.has_work() => {
            let store_names: Vec<String> = plan
                .pending_actions()
                .map(|a| a.store_id.as_str().to_string())
                .collect();
            RepairFinding {
                surface: RepairSurface::StoreMaintenance,
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
            surface: RepairSurface::StoreMaintenance,
            drift_class: DriftClass::Current,
            severity: Severity::Actionable,
            target_id: None,
            recommended_action: RepairAction::None,
            notes: None,
        },
        Err(err) => RepairFinding {
            surface: RepairSurface::StoreMaintenance,
            drift_class: DriftClass::Blocked,
            severity: Severity::Blocked,
            target_id: None,
            recommended_action: RepairAction::ManualReview,
            notes: Some(format!("Cannot evaluate storage: {err}")),
        },
    }
}

fn structural_refresh_finding(health: &ReconcileHealth) -> RepairFinding {
    let (drift, severity, action, notes) = match health {
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

    RepairFinding {
        surface: RepairSurface::StructuralRefresh,
        drift_class: drift,
        severity,
        target_id: None,
        recommended_action: action,
        notes,
    }
}

fn unsupported_surface_findings() -> [RepairFinding; 2] {
    [
        (
            RepairSurface::StaleRationale,
            "Rationale drift scoring is not yet implemented.",
        ),
        (
            RepairSurface::ExportViews,
            "Export surface is not yet implemented.",
        ),
    ]
    .map(|(surface, hint)| RepairFinding {
        surface,
        drift_class: DriftClass::Unsupported,
        severity: Severity::Unsupported,
        target_id: None,
        recommended_action: RepairAction::NotSupported,
        notes: Some(hint.to_string()),
    })
}

/// Classify the commentary overlay surface.
///
/// - No `overlay.db` on disk → `DriftClass::Absent`, no action required.
/// - Overlay present, staleness sweep runs: if any stored commentary entry
///   references a content hash that no longer matches the current graph file
///   → `DriftClass::Stale` with a `RefreshCommentary` recommendation.
/// - Overlay present with zero entries or all entries fresh → `DriftClass::Current`.
/// - If the graph is not available or the scan errors out, the finding is
///   reported as blocked so callers can distinguish "no drift" from
///   "couldn't evaluate."
fn commentary_overlay_finding(synrepo_dir: &Path) -> RepairFinding {
    use crate::store::overlay::SqliteOverlayStore;

    let overlay_dir = synrepo_dir.join("overlay");
    let overlay_db = SqliteOverlayStore::db_path(&overlay_dir);
    if !overlay_db.exists() {
        return RepairFinding {
            surface: RepairSurface::CommentaryOverlayEntries,
            drift_class: DriftClass::Absent,
            severity: Severity::ReportOnly,
            target_id: None,
            recommended_action: RepairAction::None,
            notes: Some(
                "Commentary overlay has not been materialized yet (no overlay.db).".to_string(),
            ),
        };
    }

    match scan_commentary_staleness(synrepo_dir) {
        Ok(CommentaryScan { total, stale }) if stale > 0 => RepairFinding {
            surface: RepairSurface::CommentaryOverlayEntries,
            drift_class: DriftClass::Stale,
            severity: Severity::Actionable,
            target_id: None,
            recommended_action: RepairAction::RefreshCommentary,
            notes: Some(format!(
                "{stale} of {total} commentary entries are stale against the current graph."
            )),
        },
        Ok(CommentaryScan { total, .. }) => RepairFinding {
            surface: RepairSurface::CommentaryOverlayEntries,
            drift_class: DriftClass::Current,
            severity: Severity::Actionable,
            target_id: None,
            recommended_action: RepairAction::None,
            notes: Some(format!("{total} commentary entries are current.")),
        },
        Err(err) => RepairFinding {
            surface: RepairSurface::CommentaryOverlayEntries,
            drift_class: DriftClass::Blocked,
            severity: Severity::Blocked,
            target_id: None,
            recommended_action: RepairAction::ManualReview,
            notes: Some(format!("Cannot evaluate commentary staleness: {err}")),
        },
    }
}

struct CommentaryScan {
    total: usize,
    stale: usize,
}

/// Walk every row in the `commentary` table and compare its stored
/// `source_content_hash` against the graph's current content hash for the
/// referenced node. A mismatch counts as stale; a missing node counts as
/// stale too (the pruner will eventually remove it, but until then it still
/// points at an out-of-date snapshot).
fn scan_commentary_staleness(synrepo_dir: &Path) -> crate::Result<CommentaryScan> {
    use super::commentary::resolve_commentary_node;
    use crate::core::ids::NodeId;
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
