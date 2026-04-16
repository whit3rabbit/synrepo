use std::path::Path;
use crate::config::Config;
use crate::pipeline::maintenance::MaintenancePlan;
use crate::pipeline::diagnostics::RuntimeDiagnostics;

/// Shared state for repair surface evaluation.
pub struct RepairContext<'a> {
    /// The repository root directory.
    pub repo_root: &'a Path,
    /// The `.synrepo/` directory.
    pub synrepo_dir: &'a Path,
    /// Current runtime configuration.
    pub config: &'a Config,
    /// RFC3339 timestamp when the check started.
    pub checked_at: &'a str,
    /// Collected runtime diagnostics.
    pub diagnostics: &'a RuntimeDiagnostics,
    /// Pre-computed maintenance plan.
    pub maint_plan: &'a crate::Result<MaintenancePlan>,
}

impl<'a> RepairContext<'a> {
    pub fn new(
        synrepo_dir: &'a Path,
        config: &'a Config,
        checked_at: &'a str,
        diagnostics: &'a RuntimeDiagnostics,
        maint_plan: &'a crate::Result<MaintenancePlan>,
    ) -> Self {
        let repo_root = synrepo_dir.parent().unwrap_or(synrepo_dir);
        Self {
            repo_root,
            synrepo_dir,
            config,
            checked_at,
            diagnostics,
            maint_plan,
        }
    }
}
