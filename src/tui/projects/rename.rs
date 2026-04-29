use crossterm::event::{KeyCode, KeyModifiers};

use crate::registry;

use super::GlobalAppState;

impl GlobalAppState {
    pub(super) fn handle_picker_rename_key(
        &mut self,
        code: KeyCode,
        modifiers: KeyModifiers,
    ) -> bool {
        if code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
            self.cancel_picker_rename();
            return true;
        }
        match code {
            KeyCode::Esc => {
                self.cancel_picker_rename();
                true
            }
            KeyCode::Enter => {
                self.commit_picker_rename();
                true
            }
            KeyCode::Backspace => {
                if let Some(input) = self.picker.as_mut().and_then(|p| p.rename_input.as_mut()) {
                    input.pop();
                }
                true
            }
            KeyCode::Char(ch) if !modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(input) = self.picker.as_mut().and_then(|p| p.rename_input.as_mut()) {
                    input.push(ch);
                }
                true
            }
            _ => true,
        }
    }

    fn commit_picker_rename(&mut self) {
        let selected = self.selected_project().cloned();
        let name = self
            .picker
            .as_ref()
            .and_then(|picker| picker.rename_input.clone());
        if let (Some(project), Some(name)) = (selected, name) {
            let trimmed = name.trim();
            if registry::rename_project(&project.id, trimmed).is_ok() {
                let _ = self.refresh_projects();
                if let Some(active) = self.project_states.get_mut(&project.id) {
                    active.project_name = Some(trimmed.to_string());
                    active.set_toast(format!("Renamed project to {trimmed}"));
                }
            }
        }
        self.cancel_picker_rename();
    }

    fn cancel_picker_rename(&mut self) {
        if let Some(picker) = self.picker.as_mut() {
            picker.rename_input = None;
        }
    }
}
