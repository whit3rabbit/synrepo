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

pub use self::wizard::{RepairPlan, RepairWizardOutcome, SetupPlan, SetupWizardOutcome};

pub mod actions;
pub mod app;
pub mod dashboard;
pub mod probe;
pub mod theme;
pub mod widgets;
pub mod wizard;

/// Options controlling how a TUI entry point renders and exits.
#[derive(Clone, Copy, Debug, Default)]
pub struct TuiOptions {
    /// When `true`, drop all styling even if the terminal supports color.
    pub no_color: bool,
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
}

/// Open the poll-mode dashboard on a ready repo. See `run_live_watch_dashboard`
/// for the live-mode counterpart.
pub fn run_dashboard(
    repo_root: &Path,
    integration: AgentIntegration,
    opts: TuiOptions,
) -> anyhow::Result<TuiOutcome> {
    if !stdout_is_tty() {
        return Ok(TuiOutcome::NonTtyFallback);
    }
    let theme = theme::Theme::from_no_color(opts.no_color);
    dashboard::run_poll_dashboard(repo_root, integration, theme)?;
    Ok(TuiOutcome::Exited)
}

/// Open the guided setup wizard on an uninitialized repo.
///
/// Returns a [`SetupWizardOutcome`] so the caller can execute the resulting
/// [`SetupPlan`] after the TUI alternate-screen has been torn down. The
/// library never calls the bin-side `step_*` helpers directly; the bin-side
/// dispatcher consumes the plan and runs the real I/O.
pub fn run_setup_wizard(
    repo_root: &Path,
    opts: TuiOptions,
) -> anyhow::Result<SetupWizardOutcome> {
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
/// re-runs the probe between steps per Task 11.4.
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
/// action or directly from `synrepo dashboard --integrate` in future phases.
pub fn run_integration_wizard(
    _repo_root: &Path,
    _integration: AgentIntegration,
    _opts: TuiOptions,
) -> anyhow::Result<TuiOutcome> {
    // Scaffolded; implementation lands in Phase 12.
    anyhow::bail!("integration wizard is not yet implemented")
}

/// Open the dashboard in live mode hosted by foreground `synrepo watch`.
pub fn run_live_watch_dashboard(
    _repo_root: &Path,
    _opts: TuiOptions,
) -> anyhow::Result<TuiOutcome> {
    // Scaffolded; implementation lands in Phase 8.10 / 5.5.
    anyhow::bail!("live watch dashboard is not yet implemented")
}

/// Detect whether stdout is attached to a TTY. Used by every entry point to
/// short-circuit the alt-screen path under pipe / redirect / CI.
pub fn stdout_is_tty() -> bool {
    // We intentionally avoid the `atty` crate: the stdlib path is stable in
    // recent Rust and does not pull a transitive dependency.
    use std::io::IsTerminal;
    std::io::stdout().is_terminal()
}
