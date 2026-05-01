//! Global project dashboard state and registry-backed project picker.

mod explore;
mod rename;
mod watch;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyModifiers};

use crate::bootstrap::runtime_probe::{probe, RuntimeClassification};
use crate::config::Config;
use crate::pipeline::watch::{watch_service_status, WatchServiceStatus};
use crate::pipeline::writer::live_owner_pid;
use crate::registry::{self, ProjectEntry};
use crate::tui::app::{ActiveTab, AppState};
use crate::tui::theme::Theme;

/// One registry project as shown in the global picker.
#[derive(Clone, Debug)]
pub(crate) struct ProjectRef {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) root: PathBuf,
    pub(crate) health: String,
    pub(crate) watch: String,
    pub(crate) lock: String,
    pub(crate) integration: String,
    pub(crate) last_opened_at: Option<String>,
}

impl ProjectRef {
    fn from_entry(entry: &ProjectEntry) -> Option<Self> {
        let report = probe(&entry.path);
        let health = match &report.classification {
            RuntimeClassification::Ready => "ready".to_string(),
            RuntimeClassification::Uninitialized => return None,
            RuntimeClassification::Partial { missing } => {
                format!("partial: {}", missing.len())
            }
        };
        let synrepo_dir = Config::synrepo_dir(&entry.path);
        let watch = match watch_service_status(&synrepo_dir) {
            WatchServiceStatus::Running(state) => format!("on:{}", state.pid),
            WatchServiceStatus::Starting => "starting".to_string(),
            WatchServiceStatus::Inactive => "off".to_string(),
            WatchServiceStatus::Stale(_) => "stale".to_string(),
            WatchServiceStatus::Corrupt(_) => "corrupt".to_string(),
        };
        let lock = live_owner_pid(&synrepo_dir)
            .map(|pid| format!("pid:{pid}"))
            .unwrap_or_else(|| "free".to_string());
        Some(Self {
            id: entry.effective_id(),
            name: entry.display_name(),
            root: entry.path.clone(),
            health,
            watch,
            lock,
            integration: format!("{:?}", report.agent_integration).to_lowercase(),
            last_opened_at: entry.last_opened_at.clone(),
        })
    }
}

/// Registry-backed picker state.
#[derive(Clone, Debug, Default)]
pub(crate) struct ProjectPickerState {
    pub(crate) filter: String,
    pub(crate) selected: usize,
    pub(crate) rename_input: Option<String>,
}

/// Global shell over project-scoped dashboard states.
pub(crate) struct GlobalAppState {
    pub(crate) projects: Vec<ProjectRef>,
    pub(crate) active_project_id: Option<String>,
    pub(crate) project_states: HashMap<String, AppState>,
    pub(crate) picker: Option<ProjectPickerState>,
    pub(crate) explore_selected: usize,
    pub(crate) help_visible: bool,
    pub(crate) command_palette: bool,
    pub(crate) cwd: PathBuf,
    pub(crate) theme: Theme,
    pub(crate) should_exit: bool,
}

impl GlobalAppState {
    /// Build a global shell from the user-level registry.
    pub(crate) fn new(cwd: &Path, theme: Theme, open_picker: bool) -> anyhow::Result<Self> {
        let mut state = Self {
            projects: load_project_refs()?,
            active_project_id: None,
            project_states: HashMap::new(),
            picker: open_picker.then(ProjectPickerState::default),
            explore_selected: 0,
            help_visible: false,
            command_palette: false,
            cwd: cwd.to_path_buf(),
            theme,
            should_exit: false,
        };
        if !open_picker {
            if let Some(first) = state.projects.first().cloned() {
                state.switch_project(&first.id)?;
            }
        }
        Ok(state)
    }

    pub(crate) fn active_state(&self) -> Option<&AppState> {
        let id = self.active_project_id.as_ref()?;
        self.project_states.get(id)
    }

    pub(crate) fn active_state_mut(&mut self) -> Option<&mut AppState> {
        let id = self.active_project_id.as_ref()?;
        self.project_states.get_mut(id)
    }

    /// Switch active project and lazily create its project-scoped app state.
    pub(crate) fn switch_project(&mut self, project_id: &str) -> anyhow::Result<()> {
        if let Some(active) = self.active_state_mut() {
            active.pending_explain.clear();
            active.confirm_stop_watch = None;
            active.picker = None;
        }
        let Some(project) = self.projects.iter().find(|p| p.id == project_id).cloned() else {
            anyhow::bail!("unknown project id: {project_id}");
        };
        registry::mark_project_opened(&project.id)?;
        self.active_project_id = Some(project.id.clone());
        if !self.project_states.contains_key(&project.id) {
            let report = probe(&project.root);
            let mut app =
                AppState::new_poll(&project.root, self.theme, report.agent_integration.clone());
            app.project_id = Some(project.id.clone());
            app.project_name = Some(project.name.clone());
            self.project_states.insert(project.id.clone(), app);
        }
        self.picker = None;
        self.help_visible = false;
        self.command_palette = false;
        if let Some(active) = self.active_state_mut() {
            active.pending_explain.clear();
            active.confirm_stop_watch = None;
            active.picker = None;
            active.set_toast(format!("Switched to {}", project.name));
        }
        self.refresh_projects()?;
        Ok(())
    }

    pub(crate) fn refresh_projects(&mut self) -> anyhow::Result<()> {
        self.projects = load_project_refs()?;
        Ok(())
    }

    pub(crate) fn tick(&mut self) {
        if let Some(active) = self.active_state_mut() {
            active.tick();
            self.should_exit |= active.should_exit;
        }
    }

    pub(crate) fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        if self.help_visible {
            self.help_visible = false;
            return true;
        }
        if self.command_palette {
            self.command_palette = false;
            return true;
        }
        if self
            .picker
            .as_ref()
            .and_then(|picker| picker.rename_input.as_ref())
            .is_some()
        {
            return self.handle_picker_rename_key(code, modifiers);
        }
        if self.picker.is_some() {
            return self.handle_picker_key(code, modifiers);
        }
        match code {
            KeyCode::Char('p') => {
                if self.active_state().is_some() {
                    self.open_explore_tab();
                } else {
                    let _ = self.refresh_projects();
                    self.picker = Some(ProjectPickerState::default());
                }
                true
            }
            KeyCode::Char('?') => {
                self.help_visible = true;
                true
            }
            KeyCode::Char(':') => {
                self.command_palette = true;
                true
            }
            KeyCode::Char('i') => {
                if let Some(active) = self.active_state_mut() {
                    active.set_toast("Open this project directly to run integration setup");
                }
                true
            }
            _ if self
                .active_state()
                .map(|active| matches!(active.active_tab, ActiveTab::Repos))
                .unwrap_or(false) =>
            {
                if self.handle_explore_key(code, modifiers) {
                    true
                } else {
                    self.active_state_mut()
                        .map(|active| active.handle_key(code, modifiers))
                        .unwrap_or(false)
                }
            }
            _ => self
                .active_state_mut()
                .map(|active| active.handle_key(code, modifiers))
                .unwrap_or(false),
        }
    }

    fn handle_picker_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
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
                    let _ = self.switch_project(&project.id);
                }
                true
            }
            KeyCode::Backspace => {
                if let Some(picker) = self.picker.as_mut() {
                    picker.filter.pop();
                    picker.selected = 0;
                }
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
                    }
                    Err(err) => {
                        if let Some(active) = self.active_state_mut() {
                            active.set_toast(format!("project add failed: {err}"));
                        }
                    }
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
            KeyCode::Char('d') => {
                if let Some(project) = self.selected_project().cloned() {
                    let _ = registry::remove_project(&project.root);
                    self.project_states.remove(&project.id);
                    if self.active_project_id.as_deref() == Some(project.id.as_str()) {
                        self.active_project_id = None;
                    }
                    let _ = self.refresh_projects();
                    if self.active_project_id.is_none() {
                        self.picker = Some(ProjectPickerState::default());
                    }
                }
                true
            }
            KeyCode::Char(ch) if !modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(picker) = self.picker.as_mut() {
                    picker.filter.push(ch);
                    picker.selected = 0;
                }
                true
            }
            _ => true,
        }
    }

    pub(crate) fn filtered_projects(&self) -> Vec<&ProjectRef> {
        let Some(picker) = self.picker.as_ref() else {
            return self.projects.iter().collect();
        };
        let filter = picker.filter.to_lowercase();
        self.projects
            .iter()
            .filter(|project| {
                filter.is_empty()
                    || project.name.to_lowercase().contains(&filter)
                    || project
                        .root
                        .to_string_lossy()
                        .to_lowercase()
                        .contains(&filter)
            })
            .collect()
    }

    pub(super) fn selected_project(&self) -> Option<&ProjectRef> {
        let selected = self.picker.as_ref().map(|p| p.selected).unwrap_or(0);
        self.filtered_projects().get(selected).copied()
    }
}

#[cfg(test)]
mod tests;

pub(crate) fn load_project_refs() -> anyhow::Result<Vec<ProjectRef>> {
    let mut projects: Vec<ProjectRef> = registry::load()?
        .projects
        .iter()
        .filter_map(ProjectRef::from_entry)
        .collect();
    projects.sort_by(|a, b| {
        b.last_opened_at
            .cmp(&a.last_opened_at)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.root.cmp(&b.root))
    });
    Ok(projects)
}
