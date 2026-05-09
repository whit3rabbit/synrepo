//! Apply a guided uninstall plan.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::Context;
use serde::Serialize;
use synrepo::pipeline::watch::{watch_service_status, WatchServiceStatus};

use crate::cli_support::commands::remove::hook_artifacts::{remove_agent_hook, remove_git_hook};
use crate::cli_support::commands::remove::{apply_plan, RemoveAction, RemovePlan};

use super::plan::{
    PlannedAction, ProjectUninstallPlan, SkippedItem, UninstallAction, UninstallPlan,
};

#[derive(Clone, Debug, Serialize)]
pub(crate) struct UninstallSummary {
    pub applied: Vec<AppliedUninstallAction>,
    pub manual_followups: Vec<String>,
    pub skipped: Vec<SkippedItem>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct AppliedUninstallAction {
    pub action: UninstallAction,
    pub succeeded: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Default)]
struct ProjectProgress {
    agent_candidates: BTreeSet<String>,
    agent_failures: BTreeSet<String>,
    removed_hooks: BTreeSet<String>,
    removed_agent_hooks: BTreeSet<String>,
    root_gitignore_removed: bool,
    export_gitignore_removed: bool,
}

pub(crate) fn apply_uninstall_plan(
    plan: &UninstallPlan,
    force: bool,
) -> anyhow::Result<UninstallSummary> {
    let mut summary = UninstallSummary {
        applied: Vec::new(),
        manual_followups: plan.manual_followups.clone(),
        skipped: plan.skipped.clone(),
    };

    for project in &plan.projects {
        apply_project(project, force, &mut summary)?;
    }

    let global_data_selected = plan.global.iter().any(|item| {
        item.enabled && matches!(item.action, UninstallAction::DeleteGlobalSynrepoDir { .. })
    });
    for item in plan
        .global
        .iter()
        .filter(|item| item.enabled && !matches!(item.action, UninstallAction::DeleteBinary { .. }))
    {
        apply_one(&item.action, force, &mut summary);
    }
    for item in plan
        .global
        .iter()
        .filter(|item| item.enabled && matches!(item.action, UninstallAction::DeleteBinary { .. }))
    {
        if apply_one(&item.action, force, &mut summary) && !global_data_selected {
            if let Err(err) = synrepo::registry::record_binary_uninstall() {
                tracing::warn!(error = %err, "registry update skipped after binary uninstall");
            }
        }
    }

    Ok(summary)
}

fn apply_project(
    project: &ProjectUninstallPlan,
    force: bool,
    summary: &mut UninstallSummary,
) -> anyhow::Result<()> {
    let mut selected = false;
    let mut synrepo_deleted = false;
    let mut export_deleted = false;
    let disabled_agents = project
        .actions
        .iter()
        .filter(|item| !item.enabled)
        .filter_map(|item| project_remove_tool(&item.action))
        .collect::<BTreeSet<_>>();
    let mut progress = ProjectProgress::default();

    for item in project.actions.iter().filter(|item| item.enabled) {
        selected = true;
        match &item.action {
            UninstallAction::RemoveProjectGitignoreLine { entry, .. }
                if entry == ".synrepo/" && !synrepo_deleted =>
            {
                push_failure(
                    summary,
                    item,
                    "skipped because .synrepo/ deletion did not succeed first",
                );
            }
            UninstallAction::RemoveProjectGitignoreLine { entry, .. }
                if entry != ".synrepo/" && !export_deleted =>
            {
                push_failure(
                    summary,
                    item,
                    "skipped because export directory deletion did not succeed first",
                );
            }
            _ => {
                let succeeded = apply_one(&item.action, force, summary);
                record_progress_candidate(&item.action, succeeded, &mut progress);
                if succeeded
                    && matches!(item.action, UninstallAction::DeleteProjectSynrepoDir { .. })
                {
                    synrepo_deleted = true;
                }
                if succeeded && matches!(item.action, UninstallAction::RemoveExportDir { .. }) {
                    export_deleted = true;
                }
            }
        }
    }

    if selected {
        let ProjectProgress {
            agent_candidates,
            agent_failures,
            removed_hooks,
            removed_agent_hooks,
            root_gitignore_removed,
            export_gitignore_removed,
        } = progress;
        let removed_agents = agent_candidates
            .difference(&agent_failures)
            .filter(|tool| !disabled_agents.contains(*tool))
            .cloned()
            .collect::<Vec<_>>();
        let removed_hooks = removed_hooks.into_iter().collect::<Vec<_>>();
        let removed_agent_hooks = removed_agent_hooks.into_iter().collect::<Vec<_>>();
        if let Err(err) = synrepo::registry::record_uninstall_progress(
            &project.path,
            &removed_agents,
            &removed_hooks,
            &removed_agent_hooks,
            root_gitignore_removed,
            export_gitignore_removed,
            synrepo_deleted,
        ) {
            tracing::warn!(error = %err, "registry update skipped after uninstall progress");
        }
    }
    Ok(())
}

fn project_remove_tool(action: &UninstallAction) -> Option<String> {
    match action {
        UninstallAction::ProjectRemove {
            remove_action:
                RemoveAction::DeleteShim { tool, .. } | RemoveAction::StripMcpEntry { tool, .. },
            ..
        } => Some(tool.clone()),
        _ => None,
    }
}

fn record_progress_candidate(
    action: &UninstallAction,
    succeeded: bool,
    progress: &mut ProjectProgress,
) {
    match action {
        UninstallAction::ProjectRemove {
            remove_action:
                RemoveAction::DeleteShim { tool, .. } | RemoveAction::StripMcpEntry { tool, .. },
            ..
        } => {
            if succeeded {
                progress.agent_candidates.insert(tool.clone());
            } else {
                progress.agent_failures.insert(tool.clone());
            }
        }
        UninstallAction::RemoveHook { name, .. } if succeeded => {
            progress.removed_hooks.insert(name.clone());
        }
        UninstallAction::RemoveAgentHook { tool, .. } if succeeded => {
            progress.removed_agent_hooks.insert(tool.clone());
        }
        UninstallAction::RemoveProjectGitignoreLine { entry, .. } if succeeded => {
            if entry == ".synrepo/" {
                progress.root_gitignore_removed = true;
            } else {
                progress.export_gitignore_removed = true;
            }
        }
        _ => {}
    }
}

fn apply_one(action: &UninstallAction, force: bool, summary: &mut UninstallSummary) -> bool {
    let result = match action {
        UninstallAction::ProjectRemove {
            project,
            remove_action,
        } => apply_remove_action(project, remove_action),
        UninstallAction::RemoveHook { path, mode, .. } => remove_git_hook(path, mode),
        UninstallAction::RemoveAgentHook { tool, path, .. } => remove_agent_hook(tool, path),
        UninstallAction::DeleteProjectSynrepoDir { project, path } => {
            guard_project_watch(project, path, force).and_then(|_| remove_dir(path))
        }
        UninstallAction::RemoveProjectGitignoreLine { project, entry } => {
            synrepo::bootstrap::remove_from_root_gitignore(project, entry).map(|_| ())
        }
        UninstallAction::RemoveExportDir { path, .. } => remove_dir(path),
        UninstallAction::DeleteGlobalSynrepoDir { path } => remove_dir(path),
        UninstallAction::DeleteBinary { path } => std::fs::remove_file(path)
            .with_context(|| format!("failed to delete binary {}", path.display())),
    };
    let succeeded = result.is_ok();
    summary.applied.push(AppliedUninstallAction {
        action: action.clone(),
        succeeded,
        error: result.err().map(|err| err.to_string()),
    });
    succeeded
}

fn apply_remove_action(project: &Path, action: &RemoveAction) -> anyhow::Result<()> {
    let summary = apply_plan(
        project,
        &RemovePlan {
            actions: vec![action.clone()],
            preserved: Vec::new(),
        },
    )?;
    if summary.applied.iter().all(|item| item.succeeded) {
        Ok(())
    } else {
        let error = summary
            .applied
            .into_iter()
            .find_map(|item| item.error)
            .unwrap_or_else(|| "remove action failed".to_string());
        anyhow::bail!(error)
    }
}

fn guard_project_watch(project: &Path, synrepo_dir: &Path, force: bool) -> anyhow::Result<()> {
    if !matches!(
        watch_service_status(synrepo_dir),
        WatchServiceStatus::Running(_) | WatchServiceStatus::Starting
    ) {
        return Ok(());
    }
    if force {
        eprintln!(
            "warning: watch daemon is still running for {}. Stop it with `synrepo watch stop --repo {}` for a clean teardown.",
            project.display(),
            project.display()
        );
        return Ok(());
    }
    anyhow::bail!(
        "uninstall blocked: a watch daemon is running for {}. Run `synrepo watch stop --repo {}` and retry, or pass --force.",
        project.display(),
        project.display()
    )
}

fn remove_dir(path: &Path) -> anyhow::Result<()> {
    if path.exists() {
        std::fs::remove_dir_all(path)
            .with_context(|| format!("failed to delete {}", path.display()))?;
    }
    Ok(())
}

fn push_failure(summary: &mut UninstallSummary, item: &PlannedAction, error: &str) {
    summary.applied.push(AppliedUninstallAction {
        action: item.action.clone(),
        succeeded: false,
        error: Some(error.to_string()),
    });
}
