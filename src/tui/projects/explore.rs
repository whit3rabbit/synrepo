use crossterm::event::{KeyCode, KeyModifiers};

use crate::tui::app::ActiveTab;

use super::GlobalAppState;

impl GlobalAppState {
    pub(crate) fn open_explore_tab(&mut self) {
        let active_id = self.active_project_id.clone();
        if let Some(active) = self.active_state_mut() {
            active.set_tab(ActiveTab::Explore);
        }
        if let Some(active_id) = active_id {
            self.select_explore_project(&active_id);
        }
    }

    pub(crate) fn handle_explore_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        if code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
            self.should_exit = true;
            return true;
        }
        match code {
            KeyCode::Up => {
                self.explore_selected = self.explore_selected.saturating_sub(1);
                true
            }
            KeyCode::Down => {
                let max = self.projects.len().saturating_sub(1);
                self.explore_selected = (self.explore_selected + 1).min(max);
                true
            }
            KeyCode::Enter => {
                if let Some(project_id) = self
                    .projects
                    .get(self.explore_selected_index())
                    .map(|project| project.id.clone())
                {
                    let _ = self.switch_project(&project_id);
                    self.select_explore_project(&project_id);
                }
                true
            }
            KeyCode::Char('r') => {
                let selected = self
                    .projects
                    .get(self.explore_selected_index())
                    .map(|project| project.id.clone());
                let _ = self.refresh_projects();
                if let Some(selected) = selected {
                    self.select_explore_project(&selected);
                }
                true
            }
            KeyCode::Char('w') => {
                self.toggle_explore_project_watch();
                true
            }
            _ => false,
        }
    }

    pub(crate) fn explore_selected_index(&self) -> usize {
        self.explore_selected
            .min(self.projects.len().saturating_sub(1))
    }

    fn select_explore_project(&mut self, project_id: &str) {
        if let Some(idx) = self
            .projects
            .iter()
            .position(|project| project.id == project_id)
        {
            self.explore_selected = idx;
        }
    }

    fn toggle_explore_project_watch(&mut self) {
        let selected = self
            .projects
            .get(self.explore_selected_index())
            .map(|project| project.id.clone());
        if let Some(project_id) = selected {
            self.select_explore_project(&project_id);
            self.toggle_project_watch(&project_id);
            self.select_explore_project(&project_id);
        }
    }
}
