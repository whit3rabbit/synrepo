use crossterm::event::{KeyCode, KeyModifiers};

use crate::tui::app::ActiveTab;

use super::{GlobalAppState, ProjectPickerState};

#[path = "palette_items.rs"]
mod palette_items;

#[derive(Clone, Debug, Default)]
pub(crate) struct CommandPaletteState {
    pub(crate) filter: String,
    pub(crate) selected: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CommandPaletteAction {
    ActiveKey(KeyCode),
    ActiveTabKey(ActiveTab, KeyCode),
    ProjectSwitch,
    ProjectRename,
    ProjectDetach,
    Quit,
}

#[derive(Clone, Debug)]
pub(crate) struct CommandPaletteItem {
    pub(crate) label: String,
    pub(crate) command_label: String,
    pub(crate) disabled_reason: Option<String>,
    pub(crate) requires_confirm: bool,
    pub(crate) destructive: bool,
    pub(crate) expensive: bool,
    pub(super) action: CommandPaletteAction,
}

impl CommandPaletteItem {
    pub(super) fn new(
        label: &str,
        command_label: &str,
        action: CommandPaletteAction,
        disabled_reason: Option<String>,
        requires_confirm: bool,
        destructive: bool,
        expensive: bool,
    ) -> Self {
        Self {
            label: label.to_string(),
            command_label: command_label.to_string(),
            disabled_reason,
            requires_confirm,
            destructive,
            expensive,
            action,
        }
    }

    pub(crate) fn prefix(&self) -> &'static str {
        if self.disabled_reason.is_some() {
            "x"
        } else if self.destructive {
            "!"
        } else if self.expensive {
            "~"
        } else if self.requires_confirm {
            "?"
        } else {
            " "
        }
    }
}

impl GlobalAppState {
    pub(crate) fn handle_command_palette_key(
        &mut self,
        code: KeyCode,
        modifiers: KeyModifiers,
    ) -> bool {
        if code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
            self.command_palette = None;
            return true;
        }
        match code {
            KeyCode::Esc => {
                self.command_palette = None;
                true
            }
            KeyCode::Enter => {
                match self.selected_command_palette_item() {
                    Some(item) if item.disabled_reason.is_none() => {
                        self.command_palette = None;
                        self.dispatch_command_palette_action(item.action);
                    }
                    Some(item) => {
                        let reason = item
                            .disabled_reason
                            .unwrap_or_else(|| "disabled".to_string());
                        self.set_active_toast(format!("{} disabled: {reason}", item.label));
                    }
                    None => self.set_active_toast("no matching command"),
                }
                true
            }
            KeyCode::Up => {
                if let Some(palette) = self.command_palette.as_mut() {
                    palette.selected = palette.selected.saturating_sub(1);
                }
                true
            }
            KeyCode::Down => {
                let max = self
                    .filtered_command_palette_items()
                    .len()
                    .saturating_sub(1);
                if let Some(palette) = self.command_palette.as_mut() {
                    palette.selected = (palette.selected + 1).min(max);
                }
                true
            }
            KeyCode::Backspace => {
                if let Some(palette) = self.command_palette.as_mut() {
                    palette.filter.pop();
                }
                self.clamp_command_palette_selection();
                true
            }
            KeyCode::Char(ch) if !modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(palette) = self.command_palette.as_mut() {
                    palette.filter.push(ch);
                }
                self.clamp_command_palette_selection();
                true
            }
            _ => true,
        }
    }

    pub(crate) fn filtered_command_palette_items(&self) -> Vec<CommandPaletteItem> {
        let filter = self
            .command_palette
            .as_ref()
            .map(|palette| palette.filter.to_lowercase())
            .unwrap_or_default();
        self.command_palette_items()
            .into_iter()
            .filter(|item| {
                filter.is_empty()
                    || item.label.to_lowercase().contains(&filter)
                    || item.command_label.to_lowercase().contains(&filter)
            })
            .collect()
    }

    fn selected_command_palette_item(&self) -> Option<CommandPaletteItem> {
        let selected = self
            .command_palette
            .as_ref()
            .map(|palette| palette.selected)
            .unwrap_or(0);
        self.filtered_command_palette_items()
            .into_iter()
            .nth(selected)
    }

    fn clamp_command_palette_selection(&mut self) {
        let max = self
            .filtered_command_palette_items()
            .len()
            .saturating_sub(1);
        if let Some(palette) = self.command_palette.as_mut() {
            palette.selected = palette.selected.min(max);
        }
    }

    fn dispatch_command_palette_action(&mut self, action: CommandPaletteAction) {
        match action {
            CommandPaletteAction::ActiveKey(code) => self.dispatch_active_key(code),
            CommandPaletteAction::ActiveTabKey(tab, code) => {
                if let Some(active) = self.active_state_mut() {
                    active.set_tab(tab);
                }
                self.dispatch_active_key(code);
            }
            CommandPaletteAction::ProjectSwitch => {
                let _ = self.refresh_projects();
                self.picker = Some(ProjectPickerState::default());
            }
            CommandPaletteAction::ProjectRename => {
                self.open_active_project_picker();
                if let Some(project) = self.selected_project().cloned() {
                    if let Some(picker) = self.picker.as_mut() {
                        picker.rename_input = Some(project.name);
                    }
                }
            }
            CommandPaletteAction::ProjectDetach => {
                self.open_active_project_picker();
                if let Some(project) = self.selected_project().cloned() {
                    if let Some(picker) = self.picker.as_mut() {
                        picker.detach_confirm = Some(project.id.clone());
                    }
                    self.set_active_toast(format!(
                        "detach {}: Enter to confirm, Esc to cancel",
                        project.name
                    ));
                }
            }
            CommandPaletteAction::Quit => self.should_exit = true,
        }
    }

    fn dispatch_active_key(&mut self, code: KeyCode) {
        if let Some(active) = self.active_state_mut() {
            active.handle_key(code, KeyModifiers::NONE);
            self.should_exit |= active.should_exit;
        }
    }

    fn open_active_project_picker(&mut self) {
        let active_id = self.active_project_id.clone();
        let _ = self.refresh_projects();
        self.picker = Some(ProjectPickerState::default());
        if let (Some(active_id), Some(picker)) = (active_id, self.picker.as_mut()) {
            if let Some(idx) = self
                .projects
                .iter()
                .position(|project| project.id == active_id)
            {
                picker.selected = idx;
            }
        }
    }
}
