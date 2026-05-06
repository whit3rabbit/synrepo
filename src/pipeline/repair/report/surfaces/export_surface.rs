use crate::pipeline::export::load_manifest;
use crate::pipeline::repair::{DriftClass, RepairAction, RepairFinding, RepairSurface, Severity};

use super::{RepairContext, SurfaceCheck};

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
                    "Context export has not been generated. This optional snapshot is only needed for sharing, offline review, or non-MCP agent context."
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
