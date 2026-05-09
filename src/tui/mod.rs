//! Interactive terminal surface for synrepo dashboards and wizards.

use std::path::Path;

use crate::bootstrap::runtime_probe::{probe, AgentIntegration, AgentTargetKind, Missing};
use crate::pipeline::watch::{watch_service_status, WatchServiceStatus};
use crate::tui::actions::{outcome_to_log, start_watch_daemon, ActionContext, ActionOutcome};
use crate::tui::widgets::LogEntry;

pub use self::wizard::{
    CloudCredentialSource, EmbeddingSetupChoice, ExplainChoice, ExplainWizardSupport,
    IntegrationPlan, IntegrationWizardOutcome, McpInstallPlan, McpInstallWizardOutcome, RepairPlan,
    RepairWizardOutcome, SetupFlow, SetupPlan, SetupWizardOutcome, UninstallActionKind,
    UninstallPlan, UninstallWizardOutcome,
};

pub mod actions;
pub mod agent_integrations;
pub mod app;
pub mod dashboard;
mod dashboard_tabs;
mod explain_run;
mod graph_view;
mod live_dashboard;
pub(crate) mod materializer;
pub mod mcp_status;
pub mod probe;
pub mod projects;
mod setup_flow;
pub mod theme;
mod watcher;
pub mod widgets;
pub mod wizard;

pub use graph_view::run_graph_view;
pub use setup_flow::setup_followup_needed;

/// Options controlling how a TUI entry point renders and exits.
#[derive(Clone, Copy, Debug, Default)]
pub struct TuiOptions {
    /// When `true`, drop all styling even if the terminal supports color.
    pub no_color: bool,
}

/// Dashboard-specific options.
#[derive(Clone, Copy, Debug, Default)]
pub struct DashboardOptions {
    /// Drop all styling even if the terminal supports color.
    pub no_color: bool,
    /// Seed the log pane with a single one-shot welcome entry on startup.
    pub welcome_banner: bool,
}

impl From<TuiOptions> for DashboardOptions {
    fn from(opts: TuiOptions) -> Self {
        Self {
            no_color: opts.no_color,
            welcome_banner: false,
        }
    }
}

/// Human-readable outcome of a TUI entry point.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TuiOutcome {
    /// User exited normally.
    Exited,
    /// Entry-point was skipped because stdout is not a TTY.
    NonTtyFallback,
    /// A wizard ran to completion.
    WizardCompleted,
    /// Wizard was cancelled before any writes; caller should exit zero.
    WizardCancelled,
    /// Dashboard exited with a request to launch the integration sub-wizard.
    LaunchIntegrationRequested(app::IntegrationLaunchRequest),
    /// Dashboard exited with a request to install project integration.
    LaunchProjectMcpInstallRequested,
    /// Dashboard exited with a request to launch the explain setup wizard.
    LaunchExplainSetupRequested,
    /// Dashboard exited with a request to launch the embeddings setup picker.
    LaunchEmbeddingsSetupRequested,
    /// Dashboard exited with a request to build embeddings in normal terminal
    /// output, then re-open the dashboard.
    LaunchEmbeddingBuildRequested(app::PendingEmbeddingBuild),
    /// Dashboard exited with a request to re-open on another registry project.
    SwitchProjectRequested(std::path::PathBuf),
}

/// Open the poll-mode dashboard on a ready repo. See `run_live_watch_dashboard`
/// for the live-mode counterpart.
///
/// `opts` accepts either a [`TuiOptions`] (via `.into()`) or a
/// [`DashboardOptions`] directly; the latter carries the one-shot welcome
/// banner flag that the wizard dispatcher sets after a successful setup.
pub fn run_dashboard(
    repo_root: &Path,
    integration: AgentIntegration,
    opts: impl Into<DashboardOptions>,
) -> anyhow::Result<TuiOutcome> {
    if !stdout_is_tty() {
        return Ok(TuiOutcome::NonTtyFallback);
    }
    let opts = opts.into();
    let theme = theme::Theme::from_no_color(opts.no_color);
    let startup_logs = ensure_watch_daemon_for_dashboard(repo_root);
    let intent = dashboard::run_poll_dashboard(
        repo_root,
        integration,
        theme,
        opts.welcome_banner,
        None,
        startup_logs,
    )?;
    Ok(tui_outcome(intent))
}

/// Show a short post-apply result popup. Non-TTY callers skip the popup so
/// scripted setup and integration commands keep normal terminal behavior.
pub fn run_result_popup(opts: TuiOptions, title: &str, lines: &[String]) -> anyhow::Result<()> {
    if !stdout_is_tty() {
        return Ok(());
    }
    let theme = theme::Theme::from_no_color(opts.no_color);
    wizard::run_result_popup_loop(theme, title, lines)
}

/// Open the registry-backed global project dashboard.
pub fn run_global_dashboard(
    cwd: &Path,
    opts: impl Into<DashboardOptions>,
    open_picker: bool,
) -> anyhow::Result<TuiOutcome> {
    if !stdout_is_tty() {
        return Ok(TuiOutcome::NonTtyFallback);
    }
    let opts = opts.into();
    let theme = theme::Theme::from_no_color(opts.no_color);
    Ok(tui_outcome(dashboard::run_global_dashboard(
        cwd,
        theme,
        open_picker,
    )?))
}

fn tui_outcome(intent: app::DashboardExit) -> TuiOutcome {
    match intent {
        app::DashboardExit::Quit => TuiOutcome::Exited,
        app::DashboardExit::LaunchIntegration(request) => {
            TuiOutcome::LaunchIntegrationRequested(request)
        }
        app::DashboardExit::LaunchProjectMcpInstall => TuiOutcome::LaunchProjectMcpInstallRequested,
        app::DashboardExit::LaunchExplainSetup => TuiOutcome::LaunchExplainSetupRequested,
        app::DashboardExit::LaunchEmbeddingsSetup => TuiOutcome::LaunchEmbeddingsSetupRequested,
        app::DashboardExit::LaunchEmbeddingBuild(pending) => {
            TuiOutcome::LaunchEmbeddingBuildRequested(pending)
        }
        app::DashboardExit::SwitchProject(repo_root) => {
            TuiOutcome::SwitchProjectRequested(repo_root)
        }
    }
}

fn ensure_watch_daemon_for_dashboard(repo_root: &Path) -> Vec<LogEntry> {
    let ctx = ActionContext::new(repo_root);
    match watch_service_status(&ctx.synrepo_dir) {
        WatchServiceStatus::Running(_) | WatchServiceStatus::Starting => Vec::new(),
        WatchServiceStatus::Inactive
        | WatchServiceStatus::Stale(_)
        | WatchServiceStatus::Corrupt(_) => {
            let outcome = start_watch_daemon(&ctx);
            match outcome {
                ActionOutcome::Error { .. } => vec![outcome_to_log("watch", &outcome)],
                _ => Vec::new(),
            }
        }
    }
}

/// Open the guided setup wizard on an uninitialized repo.
///
/// Returns a [`SetupWizardOutcome`] so the caller can execute the resulting
/// [`SetupPlan`] after the TUI alternate-screen has been torn down. The
/// library never calls the bin-side `step_*` helpers directly; the bin-side
/// dispatcher consumes the plan and runs the real I/O.
pub fn run_setup_wizard(repo_root: &Path, opts: TuiOptions) -> anyhow::Result<SetupWizardOutcome> {
    if !stdout_is_tty() {
        return Ok(SetupWizardOutcome::NonTty);
    }
    let theme = theme::Theme::from_no_color(opts.no_color);
    // Seed mode default from observational signal: curated when the repo has
    // concept directories populated, otherwise auto.
    let probe_report = probe(repo_root);
    let default_mode = setup_flow::default_mode(repo_root);
    let selection = setup_flow::select_setup_flow(repo_root, &probe_report);
    wizard::run_setup_wizard_loop(
        theme,
        default_mode,
        probe_report.detected_agent_targets,
        probe_report.agent_integration,
        selection.flow,
        selection.root_gitignore_present,
    )
}

/// Open the explain-only sub-wizard. Used by `synrepo setup --explain`
/// after the non-interactive setup flow has initialized the repo. Walks the
/// operator through SelectExplain → (EditCloudApiKey | SelectLocalPreset →
/// EditLocalEndpoint) → Review → Confirm and returns a
/// [`SetupWizardOutcome`]. Only the plan's `explain` field is meaningful;
/// apply-time code decides whether to patch repo-local `.synrepo/config.toml`,
/// user-scoped `~/.synrepo/config.toml`, or both.
pub fn run_explain_only_wizard(opts: TuiOptions) -> anyhow::Result<SetupWizardOutcome> {
    if !stdout_is_tty() {
        return Ok(SetupWizardOutcome::NonTty);
    }
    let theme = theme::Theme::from_no_color(opts.no_color);
    wizard::run_explain_only_wizard_loop(theme)
}

/// Open the embeddings-only setup picker. Used by the dashboard after leaving
/// the alternate screen, so provider setup has the same terminal behavior as
/// the full setup wizard.
pub fn run_embeddings_only_wizard(opts: TuiOptions) -> anyhow::Result<SetupWizardOutcome> {
    if !stdout_is_tty() {
        return Ok(SetupWizardOutcome::NonTty);
    }
    let theme = theme::Theme::from_no_color(opts.no_color);
    wizard::run_embeddings_only_wizard_loop(theme)
}

/// Open the guided repair wizard on a partial repo.
///
/// Returns a [`RepairWizardOutcome`] so the caller can execute the resulting
/// [`RepairPlan`] after the TUI alternate-screen has been torn down. The
/// library never calls the bin-side step helpers directly; the bin-side
/// dispatcher consumes the plan, runs the selected actions in order, and
/// re-runs the probe between steps.
pub fn run_repair_wizard(
    repo_root: &Path,
    _missing_override: Vec<Missing>,
    opts: TuiOptions,
) -> anyhow::Result<RepairWizardOutcome> {
    if !stdout_is_tty() {
        return Ok(RepairWizardOutcome::NonTty);
    }
    let theme = theme::Theme::from_no_color(opts.no_color);
    let probe_report = probe(repo_root);
    let missing: Vec<Missing> = match &probe_report.classification {
        crate::bootstrap::runtime_probe::RuntimeClassification::Partial { missing } => {
            missing.clone()
        }
        _ => Vec::new(),
    };
    wizard::run_repair_wizard_loop(
        theme,
        &missing,
        &probe_report.agent_integration,
        &probe_report.detected_agent_targets,
    )
}

/// Open the agent-integration sub-wizard.
pub fn run_integration_wizard(
    repo_root: &Path,
    integration: AgentIntegration,
    opts: TuiOptions,
) -> anyhow::Result<IntegrationWizardOutcome> {
    run_integration_wizard_with_initial_target(repo_root, integration, opts, None)
}

/// Open the agent-integration wizard with an optional preselected target.
pub fn run_integration_wizard_with_initial_target(
    repo_root: &Path,
    integration: AgentIntegration,
    opts: TuiOptions,
    initial_target: Option<AgentTargetKind>,
) -> anyhow::Result<IntegrationWizardOutcome> {
    if !stdout_is_tty() {
        return Ok(IntegrationWizardOutcome::NonTty);
    }
    let theme = theme::Theme::from_no_color(opts.no_color);
    let probe_report = probe(repo_root);
    wizard::run_integration_wizard_loop_with_initial_target(
        theme,
        integration,
        probe_report.detected_agent_targets,
        initial_target,
    )
}

/// Open the repo-local integration picker launched from the dashboard
/// Integrations tab.
///
/// Returns an [`McpInstallWizardOutcome`] so the caller can execute the
/// resulting project-scope MCP registration after the TUI alternate-screen has
/// been torn down.
pub fn run_mcp_install_wizard(
    repo_root: &Path,
    opts: TuiOptions,
) -> anyhow::Result<McpInstallWizardOutcome> {
    if !stdout_is_tty() {
        return Ok(McpInstallWizardOutcome::NonTty);
    }
    let theme = theme::Theme::from_no_color(opts.no_color);
    let probe_report = probe(repo_root);
    let rows = mcp_status::build_mcp_status_rows(repo_root);
    wizard::run_mcp_install_wizard_loop(theme, repo_root, rows, probe_report.detected_agent_targets)
}

/// Open the uninstall wizard for the current repo.
///
/// `installed` is the full set of detected artifacts the caller would apply
/// on a bulk `synrepo remove --apply`; `preserved` is the set of `.bak`
/// sidecars that are surfaced as guidance but are never removed.
///
/// Returns [`UninstallWizardOutcome::NonTty`] when stdout is not a terminal.
/// The bin-side dispatcher translates the resulting plan back into its own
/// `RemoveAction` list and executes it after the alt-screen has been torn
/// down, matching the pattern used by the repair and integration wizards.
pub fn run_uninstall_wizard(
    installed: Vec<UninstallActionKind>,
    preserved: Vec<std::path::PathBuf>,
    opts: TuiOptions,
) -> anyhow::Result<UninstallWizardOutcome> {
    if !stdout_is_tty() {
        return Ok(UninstallWizardOutcome::NonTty);
    }
    let theme = theme::Theme::from_no_color(opts.no_color);
    wizard::run_uninstall_wizard_loop(theme, &installed, &preserved)
}

/// Open the dashboard in live mode hosted by foreground `synrepo watch`.
///
/// Spawns the watch service on a background thread, opens the poll-mode
/// dashboard in the foreground, then (when the operator quits) sends a
/// `Stop` control request so the service releases its lease before we
/// return. The control plane is `interprocess::local_socket` (Unix socket on
/// Unix, named pipe on Windows) so this entry point is cross-platform.
pub fn run_live_watch_dashboard(repo_root: &Path, opts: TuiOptions) -> anyhow::Result<TuiOutcome> {
    if !stdout_is_tty() {
        return Ok(TuiOutcome::NonTtyFallback);
    }
    live_dashboard::run(repo_root, opts)
}

/// Detect whether stdout is attached to a TTY. Used by every entry point to
/// short-circuit the alt-screen path under pipe / redirect / CI.
pub fn stdout_is_tty() -> bool {
    #[cfg(test)]
    if let Some(is_tty) = test_stdout_is_tty_override() {
        return is_tty;
    }

    // We intentionally avoid the `atty` crate: the stdlib path is stable in
    // recent Rust and does not pull a transitive dependency.
    use std::io::IsTerminal;
    std::io::stdout().is_terminal()
}

#[cfg(test)]
thread_local! {
    static TEST_STDOUT_IS_TTY_OVERRIDE: std::cell::Cell<Option<bool>> =
        const { std::cell::Cell::new(None) };
}

#[cfg(test)]
fn test_stdout_is_tty_override() -> Option<bool> {
    TEST_STDOUT_IS_TTY_OVERRIDE.with(std::cell::Cell::get)
}

#[cfg(test)]
fn force_stdout_is_tty_for_test(is_tty: bool) -> TestStdoutIsTtyGuard {
    let previous = test_stdout_is_tty_override();
    TEST_STDOUT_IS_TTY_OVERRIDE.with(|override_slot| override_slot.set(Some(is_tty)));
    TestStdoutIsTtyGuard { previous }
}

#[cfg(test)]
struct TestStdoutIsTtyGuard {
    previous: Option<bool>,
}

#[cfg(test)]
impl Drop for TestStdoutIsTtyGuard {
    fn drop(&mut self) {
        TEST_STDOUT_IS_TTY_OVERRIDE.with(|override_slot| override_slot.set(self.previous));
    }
}

#[cfg(test)]
mod tests;
