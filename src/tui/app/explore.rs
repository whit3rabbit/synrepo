//! Single-project Repos-tab handling.

use crossterm::event::{KeyCode, KeyModifiers};

use super::AppState;
use crate::pipeline::watch::{watch_service_status, WatchServiceStatus};
use crate::tui::actions::{
    outcome_to_project_log, start_watch_daemon, stop_watch, ProjectActionContext,
};
use crate::tui::projects::{
    apply_repo_management, load_project_refs, ProjectRef, RepoManagementKey, RepoManagementPrompt,
};

impl AppState {
    pub(crate) fn ensure_explore_projects_fresh(&mut self) {
        if self.explore_projects.is_empty() {
            self.refresh_explore_projects();
        }
    }

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
                    if project.health == "missing" {
                        self.set_toast("project path is missing; press l to relink or d to detach");
                    } else {
                        self.switch_project_root = Some(project.root);
                        self.should_exit = true;
                    }
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
            KeyCode::Char('d') => {
                if let Some(project) = self.selected_explore_project() {
                    self.repo_manage_prompt = Some(RepoManagementPrompt::detach(project));
                }
                true
            }
            KeyCode::Char('n') => {
                if let Some(project) = self.selected_explore_project() {
                    self.repo_manage_prompt = Some(RepoManagementPrompt::rename(project));
                }
                true
            }
            KeyCode::Char('l') => {
                if let Some(project) = self.selected_explore_project() {
                    match RepoManagementPrompt::relink(project) {
                        Ok(prompt) => self.repo_manage_prompt = Some(prompt),
                        Err(error) => self.set_toast(format!("relink: {error}")),
                    }
                }
                true
            }
            KeyCode::Char('P') => {
                match RepoManagementPrompt::prune() {
                    Ok(Some(prompt)) => self.repo_manage_prompt = Some(prompt),
                    Ok(None) => self.set_toast("No missing projects to prune"),
                    Err(error) => self.set_toast(format!("prune preview: {error}")),
                }
                true
            }
            _ => false,
        }
    }

    pub(crate) fn handle_repo_management_key(
        &mut self,
        code: KeyCode,
        modifiers: KeyModifiers,
    ) -> bool {
        let key = self
            .repo_manage_prompt
            .as_mut()
            .unwrap()
            .key(code, modifiers);
        match key {
            RepoManagementKey::Keep => {}
            RepoManagementKey::Cancel => {
                self.repo_manage_prompt = None;
                if self.switch_project_root.is_some() {
                    self.should_exit = true;
                }
            }
            RepoManagementKey::Apply(action) => match apply_repo_management(action) {
                Ok(outcome) => {
                    self.repo_manage_prompt = None;
                    self.refresh_explore_projects();
                    if let Some(id) = outcome.selected_id.as_deref() {
                        self.select_explore_project(id);
                        if outcome.restart_current && self.project_id.as_deref() == Some(id) {
                            self.switch_project_root = self
                                .selected_explore_project()
                                .map(|project| project.root.clone());
                            self.should_exit = outcome.manual_repairs.is_empty();
                        } else if self.project_id.as_deref() == Some(id) {
                            self.project_name = self
                                .selected_explore_project()
                                .map(|project| project.name.clone());
                            self.rebuild_header_vm();
                        }
                    }
                    if self
                        .project_id
                        .as_ref()
                        .is_some_and(|id| !self.explore_projects.iter().any(|p| &p.id == id))
                    {
                        self.project_id = None;
                    }
                    self.set_toast(outcome.message.clone());
                    if !outcome.manual_repairs.is_empty() {
                        self.repo_manage_prompt = Some(RepoManagementPrompt::Notice {
                            title: outcome.message,
                            paths: outcome.manual_repairs,
                        });
                    }
                }
                Err(error) => self.set_toast(format!("project action failed: {error}")),
            },
        }
        true
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
        if project.health == "missing" {
            self.set_toast("project path is missing; relink or detach it first");
            return;
        }
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
