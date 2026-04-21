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

pub use self::app::SynthesizeMode;
pub use self::wizard::{
    CloudCredentialSource, IntegrationPlan, IntegrationWizardOutcome, RepairPlan,
    RepairWizardOutcome, SetupPlan, SetupWizardOutcome, SynthesisChoice, SynthesisWizardSupport,
    UninstallActionKind, UninstallPlan, UninstallWizardOutcome,
};

pub mod actions;
pub mod app;
pub mod dashboard;
pub mod probe;
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
    /// Dashboard exited with a request to launch the synthesis setup wizard.
    /// The caller should run `run_synthesis_only_wizard` and then re-open the
    /// dashboard.
    LaunchSynthesisSetupRequested,
    /// Dashboard exited with a request to run `synrepo synthesize` with the
    /// given scope. The caller should invoke the command function directly and
    /// then re-open the dashboard so the new snapshot is visible. When
    /// `stopped_watch` is `true`, the dashboard stopped an active watch
    /// service to free the writer lock; the caller should remind the operator
    /// that watch is no longer running after synthesis completes.
    RunSynthesizeRequested {
        /// Scope of the synthesis run.
        mode: SynthesizeMode,
        /// `true` when the dashboard stopped an active watch service.
        stopped_watch: bool,
    },
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
        app::DashboardExit::LaunchSynthesisSetup => Ok(TuiOutcome::LaunchSynthesisSetupRequested),
        app::DashboardExit::RunSynthesize {
            mode,
            stopped_watch,
        } => Ok(TuiOutcome::RunSynthesizeRequested {
            mode,
            stopped_watch,
        }),
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
    let default_mode = if has_concept_directory(repo_root) {
        Mode::Curated
    } else {
        Mode::Auto
    };
    wizard::run_setup_wizard_loop(theme, default_mode, probe_report.detected_agent_targets)
}

/// Open the synthesis-only sub-wizard. Used by `synrepo setup --synthesis`
/// after the non-interactive setup flow has initialized the repo. Walks the
/// operator through SelectSynthesis → (EditCloudApiKey | SelectLocalPreset →
/// EditLocalEndpoint) → Review → Confirm and returns a
/// [`SetupWizardOutcome`]. Only the plan's `synthesis` field is meaningful;
/// apply-time code decides whether to patch repo-local `.synrepo/config.toml`,
/// user-scoped `~/.synrepo/config.toml`, or both.
pub fn run_synthesis_only_wizard(opts: TuiOptions) -> anyhow::Result<SetupWizardOutcome> {
    if !stdout_is_tty() {
        return Ok(SetupWizardOutcome::NonTty);
    }
    let theme = theme::Theme::from_no_color(opts.no_color);
    wizard::run_synthesis_only_wizard_loop(theme)
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
            DashboardExit::LaunchSynthesisSetup => Ok(TuiOutcome::LaunchSynthesisSetupRequested),
            DashboardExit::RunSynthesize {
                mode,
                stopped_watch,
            } => Ok(TuiOutcome::RunSynthesizeRequested {
                mode,
                stopped_watch,
            }),
        }
    }
}

/// Detect whether stdout is attached to a TTY. Used by every entry point to
/// short-circuit the alt-screen path under pipe / redirect / CI.
pub fn stdout_is_tty() -> bool {
    // We intentionally avoid the `atty` crate: the stdlib path is stable in
    // recent Rust and does not pull a transitive dependency.
    use std::io::IsTerminal;
    std::io::stdout().is_terminal()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::theme::{Theme, ThemeVariant};

    #[test]
    fn tui_options_default_has_color_on() {
        let opts = TuiOptions::default();
        assert!(!opts.no_color);
    }

    #[test]
    fn no_color_flag_maps_to_plain_theme() {
        // --no-color still enters the TUI but uses Theme::plain() so no ANSI
        // codes are emitted. The theme construction is pure, so we pin the
        // mapping here without needing to actually drive a PTY.
        let theme = Theme::from_no_color(true);
        assert_eq!(theme.variant, ThemeVariant::Plain);
    }

    #[test]
    fn color_on_maps_to_dark_theme() {
        let theme = Theme::from_no_color(false);
        assert_eq!(theme.variant, ThemeVariant::Dark);
    }

    #[test]
    fn dashboard_options_from_tui_options_propagates_no_color() {
        let tui = TuiOptions { no_color: true };
        let dash: DashboardOptions = tui.into();
        assert!(dash.no_color);
        assert!(
            !dash.welcome_banner,
            "welcome_banner defaults to false when converting from TuiOptions",
        );
    }

    #[test]
    fn dashboard_options_default_has_color_on_and_no_banner() {
        let opts = DashboardOptions::default();
        assert!(!opts.no_color);
        assert!(!opts.welcome_banner);
    }

    /// In a `cargo test` harness stdout is captured, so `stdout_is_tty()`
    /// always returns `false`. That is the pipe-out path the spec refers to.
    /// The assertion here pins the short-circuit contract so a future refactor
    /// that changes the fallback signal is noticed.
    #[test]
    fn pipe_out_run_dashboard_returns_non_tty_fallback() {
        use crate::bootstrap::runtime_probe::AgentIntegration;
        let tempdir = tempfile::tempdir().unwrap();
        assert!(!stdout_is_tty(), "cargo test harness must capture stdout");
        let outcome = run_dashboard(
            tempdir.path(),
            AgentIntegration::Absent,
            TuiOptions::default(),
        )
        .expect("short-circuit is infallible");
        assert_eq!(outcome, TuiOutcome::NonTtyFallback);
    }

    #[test]
    fn pipe_out_run_setup_wizard_returns_non_tty() {
        let tempdir = tempfile::tempdir().unwrap();
        assert!(!stdout_is_tty());
        let outcome = run_setup_wizard(tempdir.path(), TuiOptions::default())
            .expect("short-circuit is infallible");
        assert_eq!(outcome, SetupWizardOutcome::NonTty);
    }

    #[test]
    fn pipe_out_run_repair_wizard_returns_non_tty() {
        let tempdir = tempfile::tempdir().unwrap();
        assert!(!stdout_is_tty());
        let outcome = run_repair_wizard(tempdir.path(), Vec::new(), TuiOptions::default())
            .expect("short-circuit is infallible");
        assert_eq!(outcome, RepairWizardOutcome::NonTty);
    }

    #[test]
    fn pipe_out_run_integration_wizard_returns_non_tty() {
        use crate::bootstrap::runtime_probe::AgentIntegration;
        let tempdir = tempfile::tempdir().unwrap();
        assert!(!stdout_is_tty());
        let outcome = run_integration_wizard(
            tempdir.path(),
            AgentIntegration::Absent,
            TuiOptions::default(),
        )
        .expect("short-circuit is infallible");
        assert_eq!(outcome, IntegrationWizardOutcome::NonTty);
    }

    #[test]
    fn ensure_watch_daemon_starts_watch_for_ready_repo_without_log_noise() {
        let _guard = crate::test_support::global_test_lock("tui-ensure-watch-daemon");
        let tempdir = tempfile::tempdir().unwrap();
        crate::bootstrap::bootstrap(tempdir.path(), None, false).expect("bootstrap");

        let logs = ensure_watch_daemon_for_dashboard(tempdir.path());
        assert!(
            logs.is_empty(),
            "successful auto-start should not seed an error log: {logs:?}"
        );
        let ctx = crate::tui::actions::ActionContext::new(tempdir.path());
        assert!(
            matches!(
                watch_service_status(&ctx.synrepo_dir),
                WatchServiceStatus::Running(_)
            ),
            "watch should be running after auto-start"
        );

        let stop = crate::tui::actions::stop_watch(&ctx);
        assert!(
            matches!(
                stop,
                crate::tui::actions::ActionOutcome::Ack { .. }
                    | crate::tui::actions::ActionOutcome::Completed { .. }
            ),
            "cleanup stop must succeed, got {stop:?}"
        );
    }

    #[test]
    fn ensure_watch_daemon_preserves_existing_running_service() {
        let _guard = crate::test_support::global_test_lock("tui-ensure-watch-daemon");
        let tempdir = tempfile::tempdir().unwrap();
        crate::bootstrap::bootstrap(tempdir.path(), None, false).expect("bootstrap");
        let ctx = crate::tui::actions::ActionContext::new(tempdir.path());
        let start = crate::tui::actions::start_watch_daemon(&ctx);
        assert!(
            matches!(start, crate::tui::actions::ActionOutcome::Ack { .. }),
            "setup start must succeed, got {start:?}"
        );

        let before_pid = match watch_service_status(&ctx.synrepo_dir) {
            WatchServiceStatus::Running(state) => state.pid,
            other => panic!("expected running watch before second ensure, got {other:?}"),
        };
        let logs = ensure_watch_daemon_for_dashboard(tempdir.path());
        assert!(
            logs.is_empty(),
            "existing watch should not emit startup logs"
        );
        let after_pid = match watch_service_status(&ctx.synrepo_dir) {
            WatchServiceStatus::Running(state) => state.pid,
            other => panic!("expected running watch after second ensure, got {other:?}"),
        };
        assert_eq!(
            before_pid, after_pid,
            "ensure must not replace the running daemon"
        );

        let stop = crate::tui::actions::stop_watch(&ctx);
        assert!(
            matches!(
                stop,
                crate::tui::actions::ActionOutcome::Ack { .. }
                    | crate::tui::actions::ActionOutcome::Completed { .. }
            ),
            "cleanup stop must succeed, got {stop:?}"
        );
    }

    #[test]
    fn ensure_watch_daemon_returns_blocked_startup_log_on_failure() {
        let tempdir = tempfile::tempdir().unwrap();
        let logs = ensure_watch_daemon_for_dashboard(tempdir.path());
        assert_eq!(
            logs.len(),
            1,
            "failed auto-start should seed one startup log"
        );
        let entry = &logs[0];
        assert_eq!(entry.tag, "watch");
        assert!(matches!(
            entry.severity,
            crate::tui::probe::Severity::Blocked
        ));
        assert!(
            entry.message.contains("not initialized"),
            "startup log should explain the failure: {:?}",
            entry.message
        );
    }
}
