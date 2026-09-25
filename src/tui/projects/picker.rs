use crossterm::event::{KeyCode, KeyModifiers};

use crate::registry;

use super::{GlobalAppState, ProjectPickerState, RepoManagementPrompt};

impl GlobalAppState {
    pub(super) fn handle_picker_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        if self
            .picker
            .as_ref()
            .and_then(|picker| picker.detach_confirm.as_ref())
            .is_some()
        {
            return self.handle_picker_detach_confirm_key(code, modifiers);
        }
        if self
            .picker
            .as_ref()
            .and_then(|picker| picker.relink_input.as_ref())
            .is_some()
        {
            return self.handle_picker_relink_key(code, modifiers);
        }
        if code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
            self.should_exit = true;
            return true;
        }
        match code {
            KeyCode::Esc => {
                if self.active_project_id.is_some() {
                    self.picker = None;
                } else {
                    self.should_exit = true;
                }
                true
            }
            KeyCode::Char('q') => {
                self.should_exit = true;
                true
            }
            KeyCode::Up => {
                if let Some(picker) = self.picker.as_mut() {
                    picker.selected = picker.selected.saturating_sub(1);
                }
                true
            }
            KeyCode::Down => {
                let max = self.filtered_projects().len().saturating_sub(1);
                if let Some(picker) = self.picker.as_mut() {
                    picker.selected = (picker.selected + 1).min(max);
                }
                true
            }
            KeyCode::Enter => {
                if let Some(project) = self.selected_project().cloned() {
                    if project.health == "missing" {
                        self.set_active_toast(
                            "project path is missing; press l to relink or d to detach",
                        );
                    } else if let Err(error) = self.switch_project(&project.id) {
                        self.set_active_toast(format!("project open failed: {error}"));
                    }
                } else {
                    self.set_active_toast("no matching project to open");
                }
                true
            }
            KeyCode::Backspace => {
                if let Some(picker) = self.picker.as_mut() {
                    picker.filter.pop();
                }
                self.clamp_picker_selection();
                true
            }
            KeyCode::Char('a') => {
                let result = (|| -> anyhow::Result<()> {
                    crate::bootstrap::bootstrap(&self.cwd, None, false)?;
                    registry::record_project(&self.cwd)?;
                    Ok(())
                })();
                match result {
                    Ok(()) => {
                        let _ = self.refresh_projects();
                        self.clamp_picker_selection();
                    }
                    Err(err) => self.set_active_toast(format!("project add failed: {err}")),
                }
                true
            }
            KeyCode::Char('r') => {
                if let Some(project) = self.selected_project().cloned() {
                    if let Some(picker) = self.picker.as_mut() {
                        picker.rename_input = Some(project.name);
                    }
                }
                true
            }
            KeyCode::Char('w') => {
                self.toggle_selected_project_watch();
                true
            }
            KeyCode::Char('l') => {
                if let Some(project) = self.selected_project().cloned() {
                    let entry = registry::resolve_project(&project.id);
                    let candidate = entry
                        .ok()
                        .and_then(|entry| {
                            let candidates = registry::relocation_candidates(&entry);
                            (candidates.len() == 1).then(|| candidates[0].display().to_string())
                        })
                        .unwrap_or_default();
                    if let Some(picker) = self.picker.as_mut() {
                        picker.relink_input = Some((project.id, candidate));
                    }
                    self.set_active_toast("enter the new project path, then press Enter");
                }
                true
            }
            KeyCode::Char('d') => {
                if let Some(project) = self.selected_project().cloned() {
                    if let Some(picker) = self.picker.as_mut() {
                        picker.detach_confirm = Some(project.id.clone());
                    }
                    self.set_active_toast(format!(
                        "detach {}: Enter to confirm, Esc to cancel",
                        project.name
                    ));
                } else {
                    self.set_active_toast("no matching project to detach");
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
            KeyCode::Char(ch) if !modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(picker) = self.picker.as_mut() {
                    picker.filter.push(ch);
                }
                self.clamp_picker_selection();
                true
            }
            _ => true,
        }
    }

    fn handle_picker_detach_confirm_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        if code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
            self.should_exit = true;
            return true;
        }
        match code {
            KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
                let project_id = self
                    .picker
                    .as_ref()
                    .and_then(|picker| picker.detach_confirm.clone());
                if let Some(project_id) = project_id {
                    self.detach_project(&project_id);
                }
                true
            }
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                if let Some(picker) = self.picker.as_mut() {
                    picker.detach_confirm = None;
                }
                self.set_active_toast("cancelled");
                true
            }
            KeyCode::Char('q') => {
                self.should_exit = true;
                true
            }
            _ => true,
        }
    }

    fn detach_project(&mut self, project_id: &str) {
        let Some(project) = self.projects.iter().find(|p| p.id == project_id).cloned() else {
            self.set_active_toast("selected project is no longer registered");
            return;
        };
        match registry::remove_project(&project.root) {
            Ok(_) => {
                self.project_states.remove(&project.id);
                if self.active_project_id.as_deref() == Some(project.id.as_str()) {
                    self.active_project_id = None;
                }
                let _ = self.refresh_projects();
                if self.active_project_id.is_none() {
                    self.picker = Some(ProjectPickerState::default());
                } else if let Some(picker) = self.picker.as_mut() {
                    picker.detach_confirm = None;
                }
                self.clamp_picker_selection();
                self.set_active_toast(format!("Detached {}", project.name));
            }
            Err(err) => self.set_active_toast(format!("project detach failed: {err}")),
        }
    }

    fn handle_picker_relink_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        if code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
            self.picker.as_mut().unwrap().relink_input = None;
            return true;
        }
        match code {
            KeyCode::Esc => self.picker.as_mut().unwrap().relink_input = None,
            KeyCode::Backspace => {
                if let Some((_, input)) = self.picker.as_mut().unwrap().relink_input.as_mut() {
                    input.pop();
                }
            }
            KeyCode::Enter => {
                let (id, input) = self.picker.as_ref().unwrap().relink_input.clone().unwrap();
                if input.trim().is_empty() {
                    self.set_active_toast("enter an existing project path");
                    return true;
                }
                match registry::relink_project(&id, std::path::Path::new(input.trim())) {
                    Ok(outcome) => {
                        self.picker.as_mut().unwrap().relink_input = None;
                        let _ = self.refresh_projects();
                        self.clamp_picker_selection();
                        self.set_active_toast(format!("Relinked {}", outcome.entry.display_name()));
                    }
                    Err(error) => self.set_active_toast(format!("project relink failed: {error}")),
                }
            }
            KeyCode::Char(ch) if !modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some((_, input)) = self.picker.as_mut().unwrap().relink_input.as_mut() {
                    input.push(ch);
                }
            }
            _ => {}
        }
        true
    }
}
