//! Setup / repair / integration wizard state machines. Each wizard is a
//! sequence of screens that only writes to disk after an explicit confirm
//! step, so cancellation at any earlier point leaves the working tree
//! byte-identical.
//!
//! This module owns the shared crossterm alt-screen lifecycle and common
//! helpers. The setup wizard lives in [`setup`]; the repair wizard in
//! [`repair`]; the integration wizard in [`integration`]. Each sub-directory
//! exports its own state machine, plan struct, and `run_*_wizard_loop` entry
//! point. The library never calls bin-side step_* helpers directly — each
//! wizard returns a plan that the bin-side dispatcher executes after the TUI
//! alternate-screen has been torn down.

use std::io::{self, Stdout};

use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::bootstrap::runtime_probe::AgentTargetKind;

pub mod integration;
pub mod repair;
pub mod setup;

pub use integration::{
    run_integration_wizard_loop, IntegrationPlan, IntegrationWizardOutcome,
    IntegrationWizardState, ActionRow as IntegrationActionRow,
};
pub use repair::{
    run_repair_wizard_loop, RepairPlan, RepairWizardOutcome, RepairWizardState,
    ActionRow as RepairActionRow, RepairActionKind, RepairStep,
};
pub use setup::{
    run_setup_wizard_loop, SetupPlan, SetupWizardOutcome, SetupWizardState, WIZARD_TARGETS,
};

/// Type alias for the crossterm-backed terminal each wizard drives.
pub(crate) type WizardTerminal = Terminal<CrosstermBackend<Stdout>>;

/// Enter raw mode + alt-screen + hide cursor. Mirror of the dashboard's
/// [`crate::tui::dashboard`] entry path; kept separate so each wizard can own
/// its own redraw cadence.
pub(crate) fn enter_tui() -> anyhow::Result<WizardTerminal> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.hide_cursor()?;
    Ok(terminal)
}

/// Tear down raw mode + alt-screen + show cursor. Safe to call multiple times.
pub(crate) fn leave_tui(terminal: &mut WizardTerminal) -> anyhow::Result<()> {
    disable_raw_mode().ok();
    execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
    terminal.show_cursor().ok();
    Ok(())
}

/// Human-readable label for an agent-target kind. Shared between the setup
/// wizard (target selection) and the repair wizard (shim row rendering).
pub(crate) fn target_label(t: AgentTargetKind) -> &'static str {
    match t {
        AgentTargetKind::Claude => "Claude Code",
        AgentTargetKind::Cursor => "Cursor",
        AgentTargetKind::Codex => "Codex CLI",
        AgentTargetKind::Copilot => "GitHub Copilot",
        AgentTargetKind::Windsurf => "Windsurf",
    }
}
