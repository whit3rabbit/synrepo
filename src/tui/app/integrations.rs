//! Integrations-tab selection and launch handling.

use crossterm::event::{KeyCode, KeyModifiers};

use super::{AppState, IntegrationLaunchRequest};
use crate::bootstrap::runtime_probe::{all_agent_targets, AgentTargetKind};

impl AppState {
    pub(crate) fn handle_integrations_key(
        &mut self,
        code: KeyCode,
        _modifiers: KeyModifiers,
    ) -> bool {
        match code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.integration_selected = self.integration_selected.saturating_sub(1);
                true
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = self.integration_display_rows.len().saturating_sub(1);
                self.integration_selected = (self.integration_selected + 1).min(max);
                true
            }
            KeyCode::Enter => {
                self.launch_selected_integration();
                true
            }
            _ => false,
        }
    }

    pub(crate) fn integration_selected_index(&self) -> usize {
        self.integration_selected
            .min(self.integration_display_rows.len().saturating_sub(1))
    }

    pub(crate) fn preserve_integration_selection(&mut self, preferred_tool: Option<&str>) {
        if let Some(tool) = preferred_tool {
            if let Some(idx) = self
                .integration_display_rows
                .iter()
                .position(|row| row.tool == tool)
            {
                self.integration_selected = idx;
                return;
            }
        }
        self.integration_selected = self.integration_selected_index();
    }

    pub(crate) fn request_integration_launch(&mut self, initial_target: Option<AgentTargetKind>) {
        self.launch_integration = Some(IntegrationLaunchRequest { initial_target });
        self.should_exit = true;
    }

    fn launch_selected_integration(&mut self) {
        let Some(row) = self
            .integration_display_rows
            .get(self.integration_selected_index())
        else {
            self.set_toast("no integration rows to select");
            return;
        };
        let Some(target) = target_for_tool(&row.tool) else {
            self.set_toast(format!("no integration wizard target for {}", row.agent));
            return;
        };
        self.request_integration_launch(Some(target));
    }
}

fn target_for_tool(tool: &str) -> Option<AgentTargetKind> {
    all_agent_targets()
        .iter()
        .copied()
        .find(|target| target.as_str() == tool)
}
