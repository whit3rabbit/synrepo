//! Shared Repos-tab prompts and registry mutations for both dashboard hosts.

use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyModifiers};

use crate::registry;

use super::ProjectRef;

#[derive(Clone, Debug)]
pub(crate) enum RepoManagementPrompt {
    Detach {
        id: String,
        name: String,
        path: PathBuf,
    },
    Relink {
        id: String,
        name: String,
        old_path: PathBuf,
        input: String,
    },
    Rename {
        id: String,
        input: String,
    },
    Prune {
        count: usize,
        sample: Vec<PathBuf>,
    },
    Notice {
        title: String,
        paths: Vec<PathBuf>,
    },
}

pub(crate) enum RepoManagementKey {
    Keep,
    Cancel,
    Apply(RepoManagementAction),
}

pub(crate) enum RepoManagementAction {
    Detach { id: String, path: PathBuf },
    Relink { id: String, path: PathBuf },
    Rename { id: String, name: String },
    Prune,
}

pub(crate) struct RepoManagementOutcome {
    pub(crate) message: String,
    pub(crate) selected_id: Option<String>,
    pub(crate) restart_current: bool,
    pub(crate) manual_repairs: Vec<PathBuf>,
}

impl RepoManagementPrompt {
    pub(crate) fn detach(project: &ProjectRef) -> Self {
        Self::Detach {
            id: project.id.clone(),
            name: project.name.clone(),
            path: project.root.clone(),
        }
    }

    pub(crate) fn relink(project: &ProjectRef) -> anyhow::Result<Self> {
        let entry = registry::resolve_project(&project.id)?;
        let candidates = registry::relocation_candidates(&entry);
        let input = if candidates.len() == 1 {
            candidates[0].display().to_string()
        } else {
            String::new()
        };
        Ok(Self::Relink {
            id: project.id.clone(),
            name: project.name.clone(),
            old_path: project.root.clone(),
            input,
        })
    }

    pub(crate) fn rename(project: &ProjectRef) -> Self {
        Self::Rename {
            id: project.id.clone(),
            input: project.name.clone(),
        }
    }

    pub(crate) fn prune() -> anyhow::Result<Option<Self>> {
        let missing: Vec<_> = registry::load()?
            .projects
            .into_iter()
            .filter(|entry| !entry.path.exists())
            .map(|entry| entry.path)
            .collect();
        if missing.is_empty() {
            return Ok(None);
        }
        Ok(Some(Self::Prune {
            count: missing.len(),
            sample: missing.into_iter().take(6).collect(),
        }))
    }

    pub(crate) fn key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> RepoManagementKey {
        if code == KeyCode::Esc
            || code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL)
        {
            return RepoManagementKey::Cancel;
        }
        match self {
            Self::Relink { id, input, .. } => match code {
                KeyCode::Backspace => {
                    input.pop();
                    RepoManagementKey::Keep
                }
                KeyCode::Enter => RepoManagementKey::Apply(RepoManagementAction::Relink {
                    id: id.clone(),
                    path: PathBuf::from(input.trim()),
                }),
                KeyCode::Char(ch) if !modifiers.contains(KeyModifiers::CONTROL) => {
                    input.push(ch);
                    RepoManagementKey::Keep
                }
                _ => RepoManagementKey::Keep,
            },
            Self::Rename { id, input } => match code {
                KeyCode::Backspace => {
                    input.pop();
                    RepoManagementKey::Keep
                }
                KeyCode::Enter => RepoManagementKey::Apply(RepoManagementAction::Rename {
                    id: id.clone(),
                    name: input.trim().to_string(),
                }),
                KeyCode::Char(ch) if !modifiers.contains(KeyModifiers::CONTROL) => {
                    input.push(ch);
                    RepoManagementKey::Keep
                }
                _ => RepoManagementKey::Keep,
            },
            Self::Detach { id, path, .. } => match code {
                KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
                    RepoManagementKey::Apply(RepoManagementAction::Detach {
                        id: id.clone(),
                        path: path.clone(),
                    })
                }
                KeyCode::Char('n') | KeyCode::Char('N') => RepoManagementKey::Cancel,
                _ => RepoManagementKey::Keep,
            },
            Self::Prune { .. } => match code {
                KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
                    RepoManagementKey::Apply(RepoManagementAction::Prune)
                }
                KeyCode::Char('n') | KeyCode::Char('N') => RepoManagementKey::Cancel,
                _ => RepoManagementKey::Keep,
            },
            Self::Notice { .. } => match code {
                KeyCode::Enter => RepoManagementKey::Cancel,
                _ => RepoManagementKey::Keep,
            },
        }
    }

    pub(crate) fn title(&self) -> &str {
        match self {
            Self::Detach { .. } => " detach project ",
            Self::Relink { .. } => " relink project ",
            Self::Rename { .. } => " rename project ",
            Self::Prune { .. } => " prune missing projects ",
            Self::Notice { .. } => " review integrations ",
        }
    }

    pub(crate) fn lines(&self) -> Vec<String> {
        match self {
            Self::Detach { name, path, .. } => vec![
                format!("Unregister {name}?"),
                path.display().to_string(),
                String::new(),
                "Repository files and .synrepo data stay in place.".to_string(),
                "Enter/y: detach    Esc/n: cancel".to_string(),
            ],
            Self::Relink {
                name,
                old_path,
                input,
                ..
            } => vec![
                format!("Move the {name} registry entry from:"),
                old_path.display().to_string(),
                "New initialized project path:".to_string(),
                input.clone(),
                String::new(),
                "Enter: relink    Esc: cancel".to_string(),
            ],
            Self::Rename { input, .. } => vec![
                "Display alias:".to_string(),
                input.clone(),
                String::new(),
                "Enter: save alias    Esc: cancel".to_string(),
            ],
            Self::Prune { count, sample } => {
                let mut lines = vec![format!("Unregister {count} missing project entries?")];
                lines.extend(sample.iter().map(|path| path.display().to_string()));
                if *count > sample.len() {
                    lines.push(format!("... and {} more", count - sample.len()));
                }
                lines.push("Repository files and .synrepo data stay in place.".to_string());
                lines.push("Enter/y: prune    Esc/n: cancel".to_string());
                lines
            }
            Self::Notice { title, paths } => {
                let mut lines = vec![title.clone(), "Review old-path references in:".to_string()];
                lines.extend(paths.iter().map(|path| path.display().to_string()));
                lines.push("Enter/Esc: close".to_string());
                lines
            }
        }
    }
}

pub(crate) fn apply_repo_management(
    action: RepoManagementAction,
) -> anyhow::Result<RepoManagementOutcome> {
    match action {
        RepoManagementAction::Detach { id, path } => {
            let current = registry::resolve_project(&id)?;
            anyhow::ensure!(
                current.path == path,
                "project path changed; refresh Repos first"
            );
            registry::remove_project(&path)?;
            Ok(RepoManagementOutcome {
                message: format!("Detached {}", current.display_name()),
                selected_id: None,
                restart_current: false,
                manual_repairs: Vec::new(),
            })
        }
        RepoManagementAction::Relink { id, path } => {
            anyhow::ensure!(!path.as_os_str().is_empty(), "enter a project path");
            let result = registry::relink_project(&id, Path::new(&path))?;
            Ok(RepoManagementOutcome {
                message: format!("Relinked {}", result.entry.display_name()),
                selected_id: Some(result.entry.id),
                restart_current: true,
                manual_repairs: result.manual_repairs,
            })
        }
        RepoManagementAction::Rename { id, name } => {
            let entry = registry::rename_project(&id, &name)?;
            Ok(RepoManagementOutcome {
                message: format!("Renamed project to {}", entry.display_name()),
                selected_id: Some(entry.id),
                restart_current: false,
                manual_repairs: Vec::new(),
            })
        }
        RepoManagementAction::Prune => {
            let removed = registry::prune_missing_projects()?;
            Ok(RepoManagementOutcome {
                message: format!("Pruned {} missing project entries", removed.len()),
                selected_id: None,
                restart_current: false,
                manual_repairs: Vec::new(),
            })
        }
    }
}
