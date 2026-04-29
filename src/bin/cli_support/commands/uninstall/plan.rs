//! Planner for `synrepo uninstall`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::Serialize;
use synrepo::config::Config;
use synrepo::registry::{self, HookEntry, ProjectEntry, Registry};

use crate::cli_support::commands::hooks::HOOK_NAMES;
use crate::cli_support::commands::remove::{build_plan as build_remove_plan, RemoveAction};

use super::binary::{classify as classify_binary, detect as detect_binary, BinaryTeardown};

#[derive(Clone, Debug, Serialize)]
pub(crate) struct UninstallPlan {
    pub projects: Vec<ProjectUninstallPlan>,
    pub global: Vec<PlannedAction>,
    pub preserved: Vec<PathBuf>,
    pub skipped: Vec<SkippedItem>,
    pub manual_followups: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ProjectUninstallPlan {
    pub path: PathBuf,
    pub exists: bool,
    pub actions: Vec<PlannedAction>,
    pub preserved: Vec<PathBuf>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct PlannedAction {
    pub action: UninstallAction,
    pub enabled: bool,
    pub destructive: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum UninstallAction {
    ProjectRemove {
        project: PathBuf,
        remove_action: RemoveAction,
    },
    RemoveHook {
        project: PathBuf,
        name: String,
        path: PathBuf,
        mode: String,
    },
    DeleteProjectSynrepoDir {
        project: PathBuf,
        path: PathBuf,
    },
    RemoveProjectGitignoreLine {
        project: PathBuf,
        entry: String,
    },
    RemoveExportDir {
        project: PathBuf,
        path: PathBuf,
    },
    DeleteGlobalSynrepoDir {
        path: PathBuf,
    },
    DeleteBinary {
        path: PathBuf,
    },
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct SkippedItem {
    pub target: String,
    pub reason: String,
}

impl UninstallPlan {
    pub(crate) fn is_empty(&self) -> bool {
        self.projects.iter().all(|p| p.actions.is_empty())
            && self.global.is_empty()
            && self.manual_followups.is_empty()
            && self.skipped.is_empty()
    }

    pub(crate) fn actions_mut(&mut self) -> impl Iterator<Item = &mut PlannedAction> {
        self.projects
            .iter_mut()
            .flat_map(|project| project.actions.iter_mut())
            .chain(self.global.iter_mut())
    }
}

pub(crate) fn build_uninstall_plan(
    repo_root: &Path,
    delete_data: bool,
    keep_binary: bool,
) -> anyhow::Result<UninstallPlan> {
    let registry = registry::load()?;
    let mut paths = registry
        .projects
        .iter()
        .map(|p| p.path.clone())
        .collect::<BTreeSet<_>>();
    paths.insert(registry::canonicalize_path(repo_root));

    let mut plan = UninstallPlan {
        projects: Vec::new(),
        global: Vec::new(),
        preserved: Vec::new(),
        skipped: Vec::new(),
        manual_followups: Vec::new(),
    };

    for path in paths {
        let entry = registry
            .projects
            .iter()
            .find(|project| project.path == path)
            .cloned();
        if !path.exists() {
            plan.skipped.push(SkippedItem {
                target: path.display().to_string(),
                reason: "registered project path is missing".to_string(),
            });
            continue;
        }
        let project = build_project_plan(&path, entry.as_ref(), delete_data)?;
        if !project.actions.is_empty() {
            plan.preserved.extend(project.preserved.clone());
            plan.projects.push(project);
        }
    }

    dedupe_shared_artifacts(&mut plan.projects);
    add_global_data(&mut plan, delete_data);
    add_binary(&mut plan, repo_root, &registry, keep_binary);
    plan.preserved.sort();
    plan.preserved.dedup();
    Ok(plan)
}

fn dedupe_shared_artifacts(projects: &mut [ProjectUninstallPlan]) {
    let mut seen = BTreeSet::new();
    for project in projects {
        project.actions.retain(|item| {
            let Some(key) = shared_artifact_key(&item.action) else {
                return true;
            };
            seen.insert(key)
        });
    }
}

fn shared_artifact_key(action: &UninstallAction) -> Option<String> {
    match action {
        UninstallAction::ProjectRemove {
            remove_action: RemoveAction::DeleteShim { tool, path },
            ..
        } => Some(format!("shim:{tool}:{}", path.display())),
        UninstallAction::ProjectRemove {
            remove_action: RemoveAction::StripMcpEntry { tool, path },
            ..
        } => Some(format!("mcp:{tool}:{}", path.display())),
        _ => None,
    }
}

fn build_project_plan(
    project: &Path,
    entry: Option<&ProjectEntry>,
    delete_data: bool,
) -> anyhow::Result<ProjectUninstallPlan> {
    let remove_plan = build_remove_plan(project, None, true)?;
    let mut actions = Vec::new();
    for action in remove_plan.actions {
        match action {
            RemoveAction::DeleteShim { .. } | RemoveAction::StripMcpEntry { .. } => {
                actions.push(enabled(UninstallAction::ProjectRemove {
                    project: project.to_path_buf(),
                    remove_action: action,
                }));
            }
            RemoveAction::RemoveGitignoreLine { .. } | RemoveAction::DeleteSynrepoDir => {}
        }
    }

    for hook in hook_actions(project, entry) {
        actions.push(enabled(hook));
    }
    add_data_actions(project, entry, delete_data, &mut actions);

    Ok(ProjectUninstallPlan {
        path: project.to_path_buf(),
        exists: project.exists(),
        actions,
        preserved: remove_plan.preserved,
    })
}

fn hook_actions(project: &Path, entry: Option<&ProjectEntry>) -> Vec<UninstallAction> {
    let mut hooks = Vec::new();
    let mut seen = BTreeSet::new();
    if let Some(entry) = entry {
        for hook in &entry.hooks {
            add_hook_action(project, hook, &mut seen, &mut hooks);
        }
    }
    for hook in scan_project_hooks(project) {
        add_hook_action(project, &hook, &mut seen, &mut hooks);
    }
    hooks
}

fn add_hook_action(
    project: &Path,
    hook: &HookEntry,
    seen: &mut BTreeSet<PathBuf>,
    hooks: &mut Vec<UninstallAction>,
) {
    let path = registry_path(project, &hook.path);
    if !path.exists() || !seen.insert(path.clone()) {
        return;
    }
    hooks.push(UninstallAction::RemoveHook {
        project: project.to_path_buf(),
        name: hook.name.clone(),
        path,
        mode: hook.mode.clone(),
    });
}

fn scan_project_hooks(project: &Path) -> Vec<HookEntry> {
    let Ok(repo) = synrepo::pipeline::git::open_repo(project) else {
        return Vec::new();
    };
    let hooks_dir = repo.git_dir().join("hooks");
    HOOK_NAMES
        .iter()
        .filter_map(|name| {
            let path = hooks_dir.join(name);
            let content = std::fs::read_to_string(&path).ok()?;
            content.contains("synrepo reconcile").then(|| HookEntry {
                name: (*name).to_string(),
                path: path.to_string_lossy().into_owned(),
                mode: "legacy".to_string(),
                installed_at: String::new(),
            })
        })
        .collect()
}

fn add_data_actions(
    project: &Path,
    entry: Option<&ProjectEntry>,
    delete_data: bool,
    actions: &mut Vec<PlannedAction>,
) {
    let synrepo_rel = entry.map(|e| e.synrepo_dir.as_str()).unwrap_or(".synrepo");
    let synrepo_dir = project.join(synrepo_rel);
    if synrepo_dir.exists() {
        actions.push(planned(
            UninstallAction::DeleteProjectSynrepoDir {
                project: project.to_path_buf(),
                path: synrepo_dir,
            },
            delete_data,
            true,
        ));
        if entry.map(|e| e.root_gitignore_entry_added).unwrap_or(false) {
            actions.push(planned(
                UninstallAction::RemoveProjectGitignoreLine {
                    project: project.to_path_buf(),
                    entry: ".synrepo/".to_string(),
                },
                delete_data,
                false,
            ));
        }
    }

    if entry
        .map(|e| e.export_gitignore_entry_added)
        .unwrap_or(false)
    {
        let export_dir = export_dir(project);
        if export_dir.exists() {
            actions.push(planned(
                UninstallAction::RemoveExportDir {
                    project: project.to_path_buf(),
                    path: export_dir,
                },
                delete_data,
                true,
            ));
        }
        actions.push(planned(
            UninstallAction::RemoveProjectGitignoreLine {
                project: project.to_path_buf(),
                entry: export_gitignore_entry(project),
            },
            delete_data,
            false,
        ));
    }
}

fn add_global_data(plan: &mut UninstallPlan, delete_data: bool) {
    let Some(registry_path) = registry::registry_path() else {
        return;
    };
    let Some(global_dir) = registry_path.parent() else {
        return;
    };
    if global_dir.exists() {
        plan.global.push(planned(
            UninstallAction::DeleteGlobalSynrepoDir {
                path: global_dir.to_path_buf(),
            },
            delete_data,
            true,
        ));
    }
}

fn add_binary(plan: &mut UninstallPlan, repo_root: &Path, registry: &Registry, keep_binary: bool) {
    let teardown = if keep_binary {
        detect_binary(repo_root, true)
    } else if let Some(binary) = &registry.binary {
        classify_binary(repo_root, &binary.path)
    } else {
        detect_binary(repo_root, false)
    };
    match teardown {
        BinaryTeardown::DeleteDirect { path } => {
            plan.global
                .push(planned(UninstallAction::DeleteBinary { path }, true, true));
        }
        BinaryTeardown::ManualCommand { command, reason } => {
            plan.manual_followups.push(format!("{command}  ({reason})"));
        }
        BinaryTeardown::Skipped { path, reason } => {
            plan.skipped.push(SkippedItem {
                target: path
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "synrepo binary".to_string()),
                reason,
            });
        }
    }
}

fn enabled(action: UninstallAction) -> PlannedAction {
    planned(action, true, false)
}

fn planned(action: UninstallAction, enabled: bool, destructive: bool) -> PlannedAction {
    PlannedAction {
        action,
        enabled,
        destructive,
    }
}

fn registry_path(repo_root: &Path, stored: &str) -> PathBuf {
    let path = PathBuf::from(stored);
    if path.is_absolute() {
        path
    } else {
        repo_root.join(path)
    }
}

fn export_dir(project: &Path) -> PathBuf {
    Config::load(project)
        .map(|config| project.join(config.export_dir))
        .unwrap_or_else(|_| project.join("synrepo-context"))
}

fn export_gitignore_entry(project: &Path) -> String {
    Config::load(project)
        .map(|config| format!("{}/", config.export_dir))
        .unwrap_or_else(|_| "synrepo-context/".to_string())
}
