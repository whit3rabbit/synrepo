use crossterm::event::{KeyCode, KeyModifiers};

use crate::tui::app::ActiveTab;

use super::{apply_repo_management, GlobalAppState, RepoManagementKey, RepoManagementPrompt};

impl GlobalAppState {
    pub(crate) fn open_explore_tab(&mut self) {
        let active_id = self.active_project_id.clone();
        if let Some(active) = self.active_state_mut() {
            active.set_tab(ActiveTab::Repos);
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
                    if let Err(error) = self.switch_project(&project_id) {
                        self.set_active_toast(format!("project open failed: {error}"));
                    } else {
                        self.select_explore_project(&project_id);
                    }
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
            KeyCode::Char('d') => {
                if let Some(project) = self.projects.get(self.explore_selected_index()) {
                    self.manage_prompt = Some(RepoManagementPrompt::detach(project));
                }
                true
            }
            KeyCode::Char('n') => {
                if let Some(project) = self.projects.get(self.explore_selected_index()) {
                    self.manage_prompt = Some(RepoManagementPrompt::rename(project));
                }
                true
            }
            KeyCode::Char('l') => {
                if let Some(project) = self.projects.get(self.explore_selected_index()) {
                    match RepoManagementPrompt::relink(project) {
                        Ok(prompt) => self.manage_prompt = Some(prompt),
                        Err(error) => self.set_active_toast(format!("relink: {error}")),
                    }
                }
                true
            }
            KeyCode::Char('P') => {
                match RepoManagementPrompt::prune() {
                    Ok(Some(prompt)) => self.manage_prompt = Some(prompt),
                    Ok(None) => self.set_active_toast("No missing projects to prune"),
                    Err(error) => self.set_active_toast(format!("prune preview: {error}")),
                }
                true
            }
            _ => false,
        }
    }

    pub(super) fn handle_manage_prompt_key(
        &mut self,
        code: KeyCode,
        modifiers: KeyModifiers,
    ) -> bool {
        let key = self.manage_prompt.as_mut().unwrap().key(code, modifiers);
        match key {
            RepoManagementKey::Keep => {}
            RepoManagementKey::Cancel => self.manage_prompt = None,
            RepoManagementKey::Apply(action) => match apply_repo_management(action) {
                Ok(outcome) => {
                    let selected_id = outcome.selected_id.clone();
                    self.manage_prompt = None;
                    if let Err(error) = self.refresh_projects() {
                        self.set_active_toast(format!("project refresh failed: {error}"));
                        return true;
                    }
                    if let Some(id) = selected_id.as_deref() {
                        self.select_explore_project(id);
                    }
                    if let Some(id) = self.active_project_id.clone() {
                        if !self.projects.iter().any(|project| project.id == id) {
                            self.active_project_id = None;
                            self.project_states.clear();
                            self.picker = Some(Default::default());
                        } else if outcome.restart_current && selected_id.as_deref() == Some(&id) {
                            self.project_states.remove(&id);
                            self.active_project_id = None;
                            if let Err(error) = self.switch_project(&id) {
                                self.set_active_toast(format!(
                                    "relinked, but open failed: {error}"
                                ));
                            } else {
                                self.open_explore_tab();
                            }
                        } else if selected_id.as_deref() == Some(&id) {
                            if let Some(name) = self
                                .projects
                                .iter()
                                .find(|project| project.id == id)
                                .map(|project| project.name.clone())
                            {
                                if let Some(active) = self.active_state_mut() {
                                    active.project_name = Some(name);
                                    active.rebuild_header_vm();
                                }
                            }
                        }
                    }
                    self.set_active_toast(outcome.message.clone());
                    if !outcome.manual_repairs.is_empty() {
                        self.manage_prompt = Some(RepoManagementPrompt::Notice {
                            title: outcome.message,
                            paths: outcome.manual_repairs,
                        });
                    }
                    self.explore_selected = self.explore_selected_index();
                }
                Err(error) => self.set_active_toast(format!("project action failed: {error}")),
            },
        }
        true
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
