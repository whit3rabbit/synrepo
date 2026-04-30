use crate::pipeline::diagnostics::{ReconcileHealth, ReconcileStaleness};
use crate::pipeline::repair::{DriftClass, RepairAction, RepairFinding, RepairSurface, Severity};

use super::{RepairContext, SurfaceCheck};

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
            ReconcileHealth::WatchStalled { last_reconcile_at } => (
                DriftClass::Stale,
                Severity::Actionable,
                RepairAction::ManualReview,
                Some(format!(
                    "Watch service holds the lease but last reconcile {last_reconcile_at} is over 1 hour old. Restart watch: `synrepo watch stop` then `synrepo watch`."
                )),
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
