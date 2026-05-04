//! Integration wizard state types.

use crossterm::event::{KeyCode, KeyModifiers};

use crate::bootstrap::runtime_probe::{AgentIntegration, AgentTargetKind};
use crate::tui::wizard::{target_tier, AgentTargetTier};

/// Plan produced by a completed integration sub-wizard.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntegrationPlan {
    /// Target selected by the operator.
    pub target: AgentTargetKind,
    /// Write (or update) the agent shim on disk.
    pub write_shim: bool,
    /// Register the synrepo MCP server in the target agent's config.
    pub register_mcp: bool,
    /// If true, overwrite an existing shim whose content differs from the
    /// canonical template. Mirrors `synrepo agent-setup --regen`. Never true
    /// by default; the operator must explicitly opt in.
    pub overwrite_shim: bool,
    /// Install local client-side synrepo nudge hooks. Supported for Codex and
    /// Claude only, and never enabled by default.
    pub install_agent_hooks: bool,
}

/// Outcome of the integration sub-wizard.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IntegrationWizardOutcome {
    /// Stdout is not a TTY; wizard was not entered.
    NonTty,
    /// Operator cancelled before confirm; no writes performed.
    Cancelled,
    /// Operator confirmed; caller must execute `plan`.
    Completed {
        /// Plan to execute.
        plan: IntegrationPlan,
    },
}

/// Steps the sub-wizard walks through.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntegrationStep {
    /// Pick agent-integration target.
    SelectTarget,
    /// Toggle which actions to run (write shim / register MCP / overwrite).
    SelectActions,
    /// Review and apply.
    Confirm,
    /// Terminal state; render loop exits.
    Complete,
}

/// Action rows shown on the SelectActions step. Stored as a small array so the
/// cursor math is obvious and testable. The "overwrite" row is always present
/// but only meaningful when the selected target already has a shim on disk.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionRow {
    /// Write the agent shim file to disk.
    WriteShim,
    /// Register the MCP server for the agent.
    RegisterMcp,
    /// Overwrite an existing shim file.
    OverwriteShim,
    /// Install local client-side synrepo nudge hooks.
    InstallAgentHooks,
}

/// Rows displayed in the action-selection step.
pub const ACTION_ROWS: &[ActionRow] = &[
    ActionRow::WriteShim,
    ActionRow::RegisterMcp,
    ActionRow::OverwriteShim,
    ActionRow::InstallAgentHooks,
];

/// State machine driving the sub-wizard. Tests drive this struct via
/// [`IntegrationWizardState::handle_key`] and assert on `finalize()` /
/// `cancelled` / `step`.
#[derive(Clone, Debug)]
pub struct IntegrationWizardState {
    /// Current step.
    pub step: IntegrationStep,
    /// Probe-derived current integration state, used to seed defaults.
    pub current: AgentIntegration,
    /// Ordered list of observationally-detected targets.
    pub detected_targets: Vec<AgentTargetKind>,
    /// Cursor in the target list (0..WIZARD_TARGETS.len()).
    pub target_cursor: usize,
    /// Committed target (set on Enter at SelectTarget).
    pub target: AgentTargetKind,
    /// Cursor in the action list (0..ACTION_ROWS.len()).
    pub action_cursor: usize,
    /// Committed action flag: write (or rewrite) the shim file for `target`.
    pub write_shim: bool,
    /// Committed action flag: register `synrepo` as an MCP server for `target`.
    pub register_mcp: bool,
    /// Committed action flag: overwrite an existing shim rather than skipping.
    pub overwrite_shim: bool,
    /// Committed action flag: install local nudge hooks for supported clients.
    pub install_agent_hooks: bool,
    /// True when the operator pressed Esc / q / Ctrl-C before Confirm.
    pub cancelled: bool,
}

impl IntegrationWizardState {
    /// Build a fresh state seeded from `current` and `detected_targets`.
    pub fn new(current: AgentIntegration, detected_targets: Vec<AgentTargetKind>) -> Self {
        // Import WIZARD_TARGETS from setup module
        use crate::tui::wizard::setup::WIZARD_TARGETS;
        // Pre-highlight the configured target when one exists, otherwise the
        // first detected target, otherwise position 0.
        let configured = current.target();
        let seed = configured.or_else(|| detected_targets.first().copied());
        let target_cursor = seed
            .and_then(|t| WIZARD_TARGETS.iter().position(|wt| wt == &t))
            .unwrap_or(0);
        let target = WIZARD_TARGETS[target_cursor];

        let (write_shim, register_mcp) = default_actions_for(&current, target);
        Self {
            step: IntegrationStep::SelectTarget,
            current,
            detected_targets,
            target_cursor,
            target,
            action_cursor: 0,
            write_shim,
            register_mcp,
            overwrite_shim: false,
            install_agent_hooks: false,
            cancelled: false,
        }
    }

    /// Handle a key event. Returns `true` when the event was consumed and the
    /// caller should redraw.
    pub fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        match self.step {
            IntegrationStep::SelectTarget => self.handle_select_target(code, modifiers),
            IntegrationStep::SelectActions => self.handle_select_actions(code, modifiers),
            IntegrationStep::Confirm => self.handle_confirm(code, modifiers),
            IntegrationStep::Complete => false,
        }
    }

    /// Shared Ctrl-C detector for per-step handlers.
    fn is_ctrl_c(code: KeyCode, modifiers: KeyModifiers) -> bool {
        code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL)
    }

    fn handle_select_target(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        use crate::tui::wizard::setup::WIZARD_TARGETS;
        // First step: Esc / q / Ctrl-C all cancel (no prior step to go back to).
        if matches!(code, KeyCode::Esc | KeyCode::Char('q')) || Self::is_ctrl_c(code, modifiers) {
            self.cancelled = true;
            self.step = IntegrationStep::Complete;
            return true;
        }
        match code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.target_cursor = self.target_cursor.saturating_sub(1);
                self.reseed_target();
                true
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.target_cursor + 1 < WIZARD_TARGETS.len() {
                    self.target_cursor += 1;
                    self.reseed_target();
                }
                true
            }
            KeyCode::Enter => {
                self.target = WIZARD_TARGETS[self.target_cursor];
                self.step = IntegrationStep::SelectActions;
                true
            }
            _ => false,
        }
    }

    fn reseed_target(&mut self) {
        use crate::tui::wizard::setup::WIZARD_TARGETS;
        self.target = WIZARD_TARGETS[self.target_cursor];
        let (w, m) = default_actions_for(&self.current, self.target);
        self.write_shim = w;
        self.register_mcp = m;
        self.overwrite_shim = false;
        self.install_agent_hooks = false;
    }

    fn handle_select_actions(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        // q and Ctrl-C cancel; Esc steps back to target selection.
        if code == KeyCode::Char('q') || Self::is_ctrl_c(code, modifiers) {
            self.cancelled = true;
            self.step = IntegrationStep::Complete;
            return true;
        }
        match code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.action_cursor = self.action_cursor.saturating_sub(1);
                true
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.action_cursor + 1 < ACTION_ROWS.len() {
                    self.action_cursor += 1;
                }
                true
            }
            KeyCode::Char(' ') => {
                self.toggle_action_at_cursor();
                true
            }
            KeyCode::Enter if self.any_action_selected() => {
                self.step = IntegrationStep::Confirm;
                true
            }
            KeyCode::Enter => {
                // Nothing selected; refuse to advance so the confirm page
                // never shows an empty plan. Keeps the UX honest.
                false
            }
            KeyCode::Esc => {
                self.step = IntegrationStep::SelectTarget;
                true
            }
            _ => false,
        }
    }

    fn toggle_action_at_cursor(&mut self) {
        match ACTION_ROWS[self.action_cursor] {
            ActionRow::WriteShim => self.write_shim = !self.write_shim,
            ActionRow::RegisterMcp => self.register_mcp = !self.register_mcp,
            ActionRow::OverwriteShim => self.overwrite_shim = !self.overwrite_shim,
            ActionRow::InstallAgentHooks if agent_hooks_supported(self.target) => {
                self.install_agent_hooks = !self.install_agent_hooks
            }
            ActionRow::InstallAgentHooks => {}
        }
    }

    fn any_action_selected(&self) -> bool {
        self.write_shim || self.register_mcp || self.install_agent_hooks
    }

    fn handle_confirm(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        match code {
            KeyCode::Enter | KeyCode::Char('y') => {
                self.step = IntegrationStep::Complete;
                true
            }
            KeyCode::Esc | KeyCode::Char('b') => {
                self.step = IntegrationStep::SelectActions;
                true
            }
            KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                self.cancelled = true;
                self.step = IntegrationStep::Complete;
                true
            }
            _ => false,
        }
    }

    /// If the state machine completed without cancelling, produce the plan.
    pub fn finalize(&self) -> Option<IntegrationPlan> {
        if self.cancelled || self.step != IntegrationStep::Complete {
            return None;
        }
        if !self.any_action_selected() {
            return None;
        }
        Some(IntegrationPlan {
            target: self.target,
            write_shim: self.write_shim,
            register_mcp: self.register_mcp,
            overwrite_shim: self.overwrite_shim,
            install_agent_hooks: self.install_agent_hooks,
        })
    }
}

/// Return true when the selected target can install local synrepo nudge hooks.
pub fn agent_hooks_supported(target: AgentTargetKind) -> bool {
    matches!(target, AgentTargetKind::Claude | AgentTargetKind::Codex)
}

/// Default action flags for a given current integration state. Partial state
/// defaults to "finish MCP registration"; Complete state defaults to nothing
/// selected (user must explicitly opt in to regen). Absent or a different
/// target defaults to "write shim and register mcp".
///
/// Shim-only targets (Cursor, Copilot, Windsurf) force `register_mcp=false`
/// by default because `step_register_mcp` will only print a manual-setup hint
/// for them. Defaulting the checkbox off avoids misleading the operator; the
/// confirm step warns anyone who explicitly opts in.
fn default_actions_for(current: &AgentIntegration, target: AgentTargetKind) -> (bool, bool) {
    let (write_shim, register_mcp) = match current {
        AgentIntegration::Complete { target: t } if *t == target => (false, false),
        AgentIntegration::Partial { target: t } if *t == target => (false, true),
        _ => (true, true),
    };
    let register_mcp = register_mcp && target_tier(target) == AgentTargetTier::Automated;
    (write_shim, register_mcp)
}
