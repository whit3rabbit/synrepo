use crate::pipeline::diagnostics::WriterStatus;
use crate::pipeline::repair::{DriftClass, RepairAction, RepairFinding, RepairSurface, Severity};

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
