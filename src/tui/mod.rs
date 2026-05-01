//! Interactive terminal surface for synrepo.
//!
//! Hosts the operator dashboard and the guided setup, repair, and integration
//! wizards. All rendering sits on `ratatui` + `crossterm`; the core runtime
//! surface is plain subcommands and remains callable without the TUI.
//!
//! Entry points used by the bare-`synrepo` router (`src/bin/cli.rs`):
//!
//! - [`run_dashboard`] — poll-mode dashboard on a `Ready` repo.
//! - [`run_setup_wizard`] — guided first-run setup for `Uninitialized` repos.
//! - [`run_repair_wizard`] — guided fixes for `Partial` repos.
//! - [`run_integration_wizard`] — agent-integration sub-flow launched from the
//!   dashboard quick action.
//! - [`run_live_watch_dashboard`] — live-mode dashboard hosted by foreground
//!   `synrepo watch` when stdout is a TTY.
//!
//! Every entry point short-circuits to a plain-text fallback (or exits
//! non-zero with a pointer to the explicit subcommand) when stdout is not a
//! TTY, so pipes, redirects, and CI are never forced into the alternate
//! screen. See the `runtime-probe` and `dashboard` specs for the contract.

use std::path::Path;

use crate::bootstrap::runtime_probe::{probe, AgentIntegration, Missing};
use crate::config::Mode;
use crate::pipeline::watch::{watch_service_status, WatchServiceStatus};
use crate::tui::actions::{outcome_to_log, start_watch_daemon, ActionContext, ActionOutcome};
use crate::tui::widgets::LogEntry;

pub use self::wizard::{
    CloudCredentialSource, ExplainChoice, ExplainWizardSupport, IntegrationPlan,
    IntegrationWizardOutcome, RepairPlan, RepairWizardOutcome, SetupPlan, SetupWizardOutcome,
    UninstallActionKind, UninstallPlan, UninstallWizardOutcome,
};

pub mod actions;
pub mod app;
pub mod dashboard;
mod dashboard_tabs;
mod explain_run;
pub(crate) mod materializer;
pub mod mcp_status;
pub mod probe;
pub mod projects;
pub mod theme;
mod watcher;
pub mod widgets;
pub mod wizard;

/// Options controlling how a TUI entry point renders and exits.
#[derive(Clone, Copy, Debug, Default)]
pub struct TuiOptions {
    /// When `true`, drop all styling even if the terminal supports color.
    pub no_color: bool,
}

/// Dashboard-specific options. Extends [`TuiOptions`] with a one-shot welcome
/// banner flag that the setup-wizard dispatcher sets on the first successful
/// wizard → dashboard transition.
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

/// Human-readable outcome of a TUI entry point. The bare-`synrepo` router
/// uses this to pick an exit code and avoid re-entering the TUI on shutdown.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TuiOutcome {
    /// User exited normally.
    Exited,
    /// Entry-point was skipped because stdout is not a TTY; a plain-text
    /// summary was printed to stdout in its place.
    NonTtyFallback,
    /// A wizard ran to completion and the caller should re-run the probe and
    /// potentially transition to the dashboard.
    WizardCompleted,
    /// Wizard was cancelled before any writes; caller should exit zero.
    WizardCancelled,
    /// Dashboard exited with a request to launch the integration sub-wizard.
    /// The caller should run `run_integration_wizard` and — on successful
    /// completion — re-open the dashboard.
    LaunchIntegrationRequested,
    /// Dashboard exited with a request to launch the explain setup wizard.
    /// The caller should run `run_explain_only_wizard` and then re-open the
    /// dashboard.
    LaunchExplainSetupRequested,
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
    match intent {
        app::DashboardExit::Quit => Ok(TuiOutcome::Exited),
        app::DashboardExit::LaunchIntegration => Ok(TuiOutcome::LaunchIntegrationRequested),
        app::DashboardExit::LaunchExplainSetup => Ok(TuiOutcome::LaunchExplainSetupRequested),
        app::DashboardExit::SwitchProject(repo_root) => {
            Ok(TuiOutcome::SwitchProjectRequested(repo_root))
        }
    }
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
    let _ = dashboard::run_global_dashboard(cwd, theme, open_picker)?;
    Ok(TuiOutcome::Exited)
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
    let default_mode = if has_concept_directory(repo_root) {
        Mode::Curated
    } else {
        Mode::Auto
    };
    wizard::run_setup_wizard_loop(theme, default_mode, probe_report.detected_agent_targets)
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

/// Detect whether the repo contains any of the canonical concept / ADR
/// directories. Used by the wizard to bias the default mode cursor.
fn has_concept_directory(repo_root: &Path) -> bool {
    ["docs/concepts", "docs/adr", "docs/decisions"]
        .iter()
        .any(|p| repo_root.join(p).is_dir())
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

/// Open the agent-integration sub-wizard. Launchable from the dashboard quick
/// action or directly via a dedicated CLI surface.
///
/// Returns an [`IntegrationWizardOutcome`] so the caller can execute the
/// resulting [`IntegrationPlan`] after the TUI alternate-screen has been torn
/// down. Like the other wizards, this library-side entry point never calls
/// bin-side step helpers directly; the caller runs the real I/O.
pub fn run_integration_wizard(
    repo_root: &Path,
    integration: AgentIntegration,
    opts: TuiOptions,
) -> anyhow::Result<IntegrationWizardOutcome> {
    if !stdout_is_tty() {
        return Ok(IntegrationWizardOutcome::NonTty);
    }
    let theme = theme::Theme::from_no_color(opts.no_color);
    let probe_report = probe(repo_root);
    wizard::run_integration_wizard_loop(theme, integration, probe_report.detected_agent_targets)
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
    live::run(repo_root, opts)
}

mod live {
    //! Live-mode shim: host the watch service on a background thread and
    //! drive the poll-mode dashboard in the foreground. Kept isolated so
    //! the unix-only plumbing does not pollute the rest of `tui::mod`.
    use std::path::Path;

    use super::{
        dashboard::run_poll_dashboard, theme::Theme, watcher::WatcherSupervisor, TuiOptions,
        TuiOutcome,
    };
    use crate::bootstrap::runtime_probe::probe as bootstrap_probe;
    use crate::tui::app::DashboardExit;

    pub(super) fn run(repo_root: &Path, opts: TuiOptions) -> anyhow::Result<TuiOutcome> {
        let theme = Theme::from_no_color(opts.no_color);
        let mut supervisor = WatcherSupervisor::new(repo_root)?;
        let event_rx = supervisor
            .start()
            .map_err(|err| anyhow::anyhow!(err.to_string()))?;

        // Re-probe so the dashboard header reflects agent integration state.
        let report = bootstrap_probe(repo_root);
        // Live mode: the log pane drains `WatchEvent`s from `event_rx`;
        // header stats still come from the status-snapshot file refresh.
        let intent = run_poll_dashboard(
            repo_root,
            report.agent_integration,
            theme,
            false,
            Some(event_rx),
            Vec::new(),
        )?;

        // Dashboard has exited. Stop the service.
        supervisor.stop();

        match intent {
            DashboardExit::Quit => Ok(TuiOutcome::Exited),
            DashboardExit::LaunchIntegration => Ok(TuiOutcome::LaunchIntegrationRequested),
            DashboardExit::LaunchExplainSetup => Ok(TuiOutcome::LaunchExplainSetupRequested),
            DashboardExit::SwitchProject(repo_root) => {
                Ok(TuiOutcome::SwitchProjectRequested(repo_root))
            }
        }
    }
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
