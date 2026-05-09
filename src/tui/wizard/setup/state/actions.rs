use crossterm::event::{KeyCode, KeyModifiers};

use super::{SetupActionRow, SetupFlow, SetupStep, SetupWizardState, SETUP_ACTION_ROWS};
use crate::bootstrap::runtime_probe::AgentTargetKind;
use crate::tui::wizard::{target_tier, AgentTargetTier};

impl SetupWizardState {
    pub(super) fn handle_select_actions_key(
        &mut self,
        code: KeyCode,
        modifiers: KeyModifiers,
    ) -> bool {
        if code == KeyCode::Char('q')
            || (code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL))
        {
            return self.cancel_to_complete();
        }
        match code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.action_cursor = self.action_cursor.saturating_sub(1);
                true
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.action_cursor + 1 < SETUP_ACTION_ROWS.len() {
                    self.action_cursor += 1;
                }
                true
            }
            KeyCode::Char(' ') => {
                self.toggle_action_at_cursor();
                true
            }
            KeyCode::Enter => {
                self.step = match self.flow {
                    SetupFlow::Full => SetupStep::SelectEmbeddings,
                    SetupFlow::FollowUp => SetupStep::Confirm,
                };
                true
            }
            KeyCode::Esc | KeyCode::Char('b') => {
                self.step = SetupStep::SelectTarget;
                true
            }
            _ => false,
        }
    }

    pub(super) fn reseed_target_actions(&mut self) {
        let (write_agent_shim, register_mcp) =
            super::default_agent_actions_for(&self.current_integration, self.target);
        self.write_agent_shim = write_agent_shim;
        self.register_mcp = register_mcp;
        self.install_agent_hooks = false;
    }

    fn toggle_action_at_cursor(&mut self) {
        match SETUP_ACTION_ROWS[self.action_cursor] {
            SetupActionRow::AddRootGitignore => self.add_root_gitignore = !self.add_root_gitignore,
            SetupActionRow::WriteAgentShim if self.target.is_some() => {
                self.write_agent_shim = !self.write_agent_shim
            }
            SetupActionRow::RegisterMcp if self.target_can_register_mcp() => {
                self.register_mcp = !self.register_mcp
            }
            SetupActionRow::InstallAgentHooks if self.target_supports_hooks() => {
                self.install_agent_hooks = !self.install_agent_hooks
            }
            SetupActionRow::WriteAgentShim
            | SetupActionRow::RegisterMcp
            | SetupActionRow::InstallAgentHooks => {}
        }
    }

    pub(crate) fn target_supports_hooks(&self) -> bool {
        matches!(
            self.target,
            Some(AgentTargetKind::Claude | AgentTargetKind::Codex)
        )
    }

    pub(crate) fn target_can_register_mcp(&self) -> bool {
        self.target
            .is_some_and(|target| target_tier(target) == AgentTargetTier::Automated)
    }
}
