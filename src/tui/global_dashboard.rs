use std::path::{Path, PathBuf};

use super::{dashboard, stdout_is_tty, theme, DashboardOptions, TuiOutcome};

/// Global dashboard result plus the project active when it exited.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GlobalDashboardOutcome {
    /// Dashboard outcome requested by the active global project state.
    pub outcome: TuiOutcome,
    /// Active project root at exit time, when a project was selected.
    pub active_root: Option<PathBuf>,
}

/// Open the registry-backed global project dashboard.
pub fn run_global_dashboard(
    cwd: &Path,
    opts: impl Into<DashboardOptions>,
    open_picker: bool,
) -> anyhow::Result<TuiOutcome> {
    Ok(run_global_dashboard_with_active_project(cwd, opts, open_picker)?.outcome)
}

/// Open the global dashboard and preserve the active project for sub-actions.
pub fn run_global_dashboard_with_active_project(
    cwd: &Path,
    opts: impl Into<DashboardOptions>,
    open_picker: bool,
) -> anyhow::Result<GlobalDashboardOutcome> {
    if !stdout_is_tty() {
        return Ok(GlobalDashboardOutcome {
            outcome: TuiOutcome::NonTtyFallback,
            active_root: None,
        });
    }
    let opts = opts.into();
    let theme = theme::Theme::from_no_color(opts.no_color);
    let (intent, active_root) =
        dashboard::run_global_dashboard_with_active_project(cwd, theme, open_picker)?;
    Ok(GlobalDashboardOutcome {
        outcome: super::tui_outcome(intent),
        active_root,
    })
}
