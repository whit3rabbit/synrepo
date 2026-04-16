mod context;
pub mod surfaces;

pub use context::RepairContext;

use std::path::Path;

use crate::config::Config;
use crate::pipeline::{
    diagnostics::collect_diagnostics,
    maintenance::{plan_maintenance, MaintenancePlan},
    writer::now_rfc3339,
};

use super::{RepairFinding, RepairReport, RepairSurface};

/// A pluggable logic unit for evaluating drift on a specific sub-surface of
/// the repository.
pub trait SurfaceCheck {
    /// The surface this check is responsible for.
    fn surface(&self) -> RepairSurface;

    /// Evaluate the surface and return zero or more findings.
    ///
    /// Probes should avoid redundant work by utilizing the shared `RepairContext`.
    /// If an evaluation fails due to an environmental or internal error, it
    /// MUST return an explicit `DriftClass::Blocked` finding rather than
    /// silently omitting findings.
    fn evaluate(&self, ctx: &RepairContext) -> Vec<RepairFinding>;
}

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
    let diag = collect_diagnostics(synrepo_dir, config);
    let ctx = RepairContext::new(synrepo_dir, config, &now, &diag, maint_plan);

    let mut findings = Vec::new();

    // The ordered registry of repair surfaces.
    let checks: &[&dyn SurfaceCheck] = &[
        &surfaces::WriterLockCheck,
        &surfaces::StoreMaintenanceCheck,
        &surfaces::StructuralRefreshCheck,
        &surfaces::DeclaredLinksCheck,
        &surfaces::CommentaryOverlayCheck,
        &surfaces::ExportSurfaceCheck,
        &surfaces::EdgeDriftCheck,
        &surfaces::RetiredObservationsCheck,
        &surfaces::ProposedLinksOverlayCheck,
        &surfaces::UnsupportedSurfaceCheck,
    ];

    for check in checks {
        findings.extend(check.evaluate(&ctx));
    }

    RepairReport {
        checked_at: ctx.checked_at.to_string(),
        findings,
    }
}
