//! Live-mode dashboard host.

use std::path::Path;

use crate::bootstrap::runtime_probe::probe as bootstrap_probe;
use crate::tui::app::DashboardExit;
use crate::tui::dashboard::run_poll_dashboard;
use crate::tui::theme::Theme;
use crate::tui::watcher::WatcherSupervisor;
use crate::tui::{TuiOptions, TuiOutcome};

/// Host the watch service on a background thread and drive the poll-mode
/// dashboard in the foreground.
pub(super) fn run(repo_root: &Path, opts: TuiOptions) -> anyhow::Result<TuiOutcome> {
    let theme = Theme::from_no_color(opts.no_color);
    let mut supervisor = WatcherSupervisor::new(repo_root)?;
    let event_rx = supervisor
        .start()
        .map_err(|err| anyhow::anyhow!(err.to_string()))?;

    let report = bootstrap_probe(repo_root);
    let intent = run_poll_dashboard(
        repo_root,
        report.agent_integration,
        theme,
        false,
        Some(event_rx),
        Vec::new(),
    )?;

    supervisor.stop();

    match intent {
        DashboardExit::Quit => Ok(TuiOutcome::Exited),
        DashboardExit::LaunchIntegration => Ok(TuiOutcome::LaunchIntegrationRequested),
        DashboardExit::LaunchProjectMcpInstall => Ok(TuiOutcome::LaunchProjectMcpInstallRequested),
        DashboardExit::LaunchExplainSetup => Ok(TuiOutcome::LaunchExplainSetupRequested),
        DashboardExit::LaunchEmbeddingsSetup => Ok(TuiOutcome::LaunchEmbeddingsSetupRequested),
        DashboardExit::LaunchEmbeddingBuild(pending) => {
            Ok(TuiOutcome::LaunchEmbeddingBuildRequested(pending))
        }
        DashboardExit::SwitchProject(repo_root) => {
            Ok(TuiOutcome::SwitchProjectRequested(repo_root))
        }
    }
}
