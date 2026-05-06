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
pub mod mcp_install;
pub mod repair;
pub mod setup;
pub mod uninstall;

pub use integration::{
    run_integration_wizard_loop, ActionRow as IntegrationActionRow, IntegrationPlan,
    IntegrationWizardOutcome, IntegrationWizardState,
};
pub use mcp_install::{
    run_mcp_install_wizard_loop, McpInstallPlan, McpInstallWizardOutcome, McpInstallWizardState,
};
pub use repair::{
    run_repair_wizard_loop, ActionRow as RepairActionRow, RepairActionKind, RepairPlan, RepairStep,
    RepairWizardOutcome, RepairWizardState,
};
pub use setup::{
    run_explain_only_wizard_loop, run_setup_wizard_loop, CloudCredentialSource, ExplainChoice,
    ExplainWizardSupport, SetupPlan, SetupWizardOutcome, SetupWizardState, WIZARD_TARGETS,
};
pub use uninstall::{
    run_uninstall_wizard_loop, UninstallActionKind, UninstallActionRow, UninstallPlan,
    UninstallStep, UninstallWizardOutcome, UninstallWizardState,
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
/// wizard (target selection) and the repair wizard.
pub(crate) fn target_label(t: AgentTargetKind) -> &'static str {
    match t {
        AgentTargetKind::Amp => "Amp",
        AgentTargetKind::Antigravity => "Google Antigravity",
        AgentTargetKind::Claude => "Claude Code",
        AgentTargetKind::Cline => "Cline",
        AgentTargetKind::CodeBuddy => "CodeBuddy CLI",
        AgentTargetKind::Cursor => "Cursor",
        AgentTargetKind::Copilot => "GitHub Copilot",
        AgentTargetKind::Crush => "Charm Crush",
        AgentTargetKind::Codex => "Codex CLI",
        AgentTargetKind::Forge => "Forge",
        AgentTargetKind::Gemini => "Gemini CLI",
        AgentTargetKind::Hermes => "Hermes",
        AgentTargetKind::Iflow => "iFlow CLI",
        AgentTargetKind::Junie => "Junie",
        AgentTargetKind::Kilocode => "Kilo Code",
        AgentTargetKind::Opencode => "OpenCode",
        AgentTargetKind::Openclaw => "OpenClaw",
        AgentTargetKind::Pi => "Pi",
        AgentTargetKind::Qodercli => "Qoder CLI",
        AgentTargetKind::Qwen => "Qwen Code",
        AgentTargetKind::Roo => "Roo Code",
        AgentTargetKind::Tabnine => "Tabnine CLI",
        AgentTargetKind::Trae => "Trae",
        AgentTargetKind::Windsurf => "Windsurf",
    }
}

/// User-facing noun for the artifact this target writes — "skill" for tools
/// that follow the Agent Skills standard (`SKILL.md`) and "instructions"
/// otherwise. Mirrors `AgentTool::artifact_label()` for the wizard-visible
/// subset so the TUI can render honest copy without depending on the binary
/// crate's `AgentTool` enum.
pub(crate) fn target_artifact_label(t: AgentTargetKind) -> &'static str {
    if agent_config::skill_by_id(t.as_str()).is_some() {
        "skill"
    } else {
        "instructions"
    }
}

/// Same as [`target_artifact_label`] but keyed on the canonical-name string
/// the binary persists in the install registry (so removal/uninstall flows
/// can render honest copy when they only have the string in hand). Falls
/// back to "instructions" for unrecognized names — a safer default than
/// "skill" because instructions-style files are the more generic shape.
/// Keep the SKILL-tier list in sync with `AgentTool::artifact_label()`.
pub(crate) fn artifact_label_for_canonical(tool: &str) -> &'static str {
    match tool {
        "claude" | "cursor" | "codex" | "windsurf" | "gemini" | "trae" => "skill",
        _ => "instructions",
    }
}

/// Two-tier support matrix mirrored on the library side so the TUI can render
/// honest labels without a dependency on the binary crate's `AgentTool` enum.
/// Must stay in sync with `AgentTool::automation_tier()` in the binary; the
/// `automation_tier_matches_step_register_mcp_dispatch` test enforces that
/// agreement for every wizard target.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentTargetTier {
    /// `synrepo setup` writes the agent's MCP config entry automatically.
    Automated,
    /// `synrepo setup` writes the shim only; MCP wiring is manual.
    ShimOnly,
}

/// Tier for a wizard-visible agent target. Of the five observationally-
/// detectable targets, only Claude and Codex ship automated MCP registration
/// today. OpenCode is also Automated on the CLI, but is not part of the
/// wizard probe set, so it does not appear here.
pub fn target_tier(t: AgentTargetKind) -> AgentTargetTier {
    if agent_config::mcp_by_id(t.as_str())
        .map(|installer| {
            installer
                .supported_mcp_scopes()
                .contains(&agent_config::ScopeKind::Local)
        })
        .unwrap_or(false)
    {
        AgentTargetTier::Automated
    } else {
        AgentTargetTier::ShimOnly
    }
}
