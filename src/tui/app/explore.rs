//! Single-project Repos-tab handling.

use crossterm::event::{KeyCode, KeyModifiers};

use super::AppState;
use crate::pipeline::watch::{watch_service_status, WatchServiceStatus};
use crate::tui::actions::{
    outcome_to_project_log, start_watch_daemon, stop_watch, ProjectActionContext,
};
use crate::tui::projects::{load_project_refs, ProjectRef};

impl AppState {
    pub(crate) fn refresh_explore_projects(&mut self) {
        let selected = self
            .explore_projects
            .get(self.explore_selected_index())
            .map(|project| project.id.clone());
        self.explore_projects = load_project_refs().unwrap_or_default();
        if let Some(selected) = selected {
            self.select_explore_project(&selected);
        } else if let Some(project_id) = self.project_id.clone() {
            self.select_explore_project(&project_id);
        } else if let Some(idx) = self
            .explore_projects
            .iter()
            .position(|project| project.root == self.repo_root)
        {
            self.explore_selected = idx;
        }
    }

    pub(crate) fn handle_explore_key(&mut self, code: KeyCode, _modifiers: KeyModifiers) -> bool {
        match code {
            KeyCode::Up => {
                self.explore_selected = self.explore_selected.saturating_sub(1);
                true
            }
            KeyCode::Down => {
                let max = self.explore_projects.len().saturating_sub(1);
                self.explore_selected = (self.explore_selected + 1).min(max);
                true
            }
            KeyCode::Enter => {
                if let Some(project) = self.selected_explore_project().cloned() {
                    self.switch_project_root = Some(project.root);
                    self.should_exit = true;
                }
                true
            }
            KeyCode::Char('r') => {
                self.refresh_explore_projects();
                self.set_toast("repos refreshed");
                true
            }
            KeyCode::Char('w') => {
                self.toggle_explore_watch();
                true
            }
            _ => false,
        }
    }

    pub(crate) fn explore_selected_index(&self) -> usize {
        self.explore_selected
            .min(self.explore_projects.len().saturating_sub(1))
    }

    fn selected_explore_project(&self) -> Option<&ProjectRef> {
        self.explore_projects.get(self.explore_selected_index())
    }

    fn select_explore_project(&mut self, project_id: &str) {
        if let Some(idx) = self
            .explore_projects
            .iter()
            .position(|project| project.id == project_id)
        {
            self.explore_selected = idx;
        }
    }

    fn toggle_explore_watch(&mut self) {
        let Some(project) = self.selected_explore_project().cloned() else {
            return;
        };
        let ctx = ProjectActionContext::new(&project.id, &project.name, &project.root);
        let action_ctx = ctx.action_context();
        let outcome = match watch_service_status(&ctx.synrepo_dir) {
            WatchServiceStatus::Inactive => start_watch_daemon(&action_ctx),
            WatchServiceStatus::Running(_)
            | WatchServiceStatus::Starting
            | WatchServiceStatus::Stale(_)
            | WatchServiceStatus::Corrupt(_) => stop_watch(&action_ctx),
        };
        let entry = outcome_to_project_log(&ctx, "watch", &outcome);
        self.set_toast(entry.message.clone());
        self.log.push(entry);
        self.refresh_explore_projects();
    }
}
