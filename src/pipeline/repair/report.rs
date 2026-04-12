use std::path::Path;

use crate::{
    config::Config,
    pipeline::maintenance::{plan_maintenance, MaintenancePlan},
};

use super::{
    declared_links::check_declared_links,
    DriftClass, RepairAction, RepairFinding, RepairReport, RepairSurface, Severity,
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

fn unsupported_surface_findings() -> [RepairFinding; 3] {
    [
        (
            RepairSurface::StaleRationale,
            "Rationale drift scoring is not yet implemented.",
        ),
        (
            RepairSurface::OverlayEntries,
            "Overlay surface is not yet implemented (phase 4+).",
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
