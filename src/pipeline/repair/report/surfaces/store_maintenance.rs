use crate::pipeline::repair::{DriftClass, RepairAction, RepairFinding, RepairSurface, Severity};

use super::{RepairContext, SurfaceCheck};

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
