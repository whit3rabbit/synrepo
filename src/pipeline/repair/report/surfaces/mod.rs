mod commentary;
mod cross_links;
mod drift;
mod rationale;

use crate::pipeline::diagnostics::{ReconcileHealth, ReconcileStaleness, WriterStatus};
use crate::pipeline::export::load_manifest;
use crate::pipeline::repair::{
    declared_links::check_declared_links, DriftClass, RepairAction, RepairFinding, RepairSurface,
    Severity,
};

use super::{RepairContext, SurfaceCheck};

pub use commentary::CommentaryOverlayCheck;
pub use cross_links::ProposedLinksOverlayCheck;
pub use drift::{EdgeDriftCheck, RetiredObservationsCheck};
pub use rationale::StaleRationaleCheck;

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
            WriterStatus::Corrupt(e) => RepairFinding {
                surface: self.surface(),
                drift_class: DriftClass::Corrupted,
                severity: Severity::Blocked,
                target_id: None,
                recommended_action: RepairAction::ManualReview,
                notes: Some(format!("Writer lock file is corrupt: {e}. Remove .synrepo/state/writer.lock to recover.")),
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
            ReconcileHealth::Stale(ReconcileStaleness::Outcome(last_outcome)) => (
                DriftClass::Stale,
                Severity::Actionable,
                RepairAction::RunReconcile,
                Some(format!("Last reconcile outcome: {last_outcome}")),
            ),
            ReconcileHealth::Stale(ReconcileStaleness::Age { .. }) => (
                DriftClass::Stale,
                Severity::Actionable,
                RepairAction::RunReconcile,
                Some("Last successful reconcile was over 1 hour ago.".to_string()),
            ),
            ReconcileHealth::Corrupt(e) => (
                DriftClass::Corrupted,
                Severity::Blocked,
                RepairAction::ManualReview,
                Some(format!(
                    "Reconcile state file is corrupt: {e}. Remove .synrepo/state/reconcile-state.json to recover."
                )),
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
                    "Export directory has not been generated yet. Run `synrepo export`."
                        .to_string(),
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
