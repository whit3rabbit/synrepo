//! Apply a guided uninstall plan.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::Context;
use serde::Serialize;
use synrepo::pipeline::watch::{watch_service_status, WatchServiceStatus};

use crate::cli_support::commands::hooks::{full_hook_script, HOOK_BEGIN, HOOK_END};
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
    let mut agent_candidates = BTreeSet::new();
    let mut agent_failures = BTreeSet::new();
    let mut removed_hooks = BTreeSet::new();
    let mut root_gitignore_removed = false;
    let mut export_gitignore_removed = false;

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
                record_progress_candidate(
                    &item.action,
                    succeeded,
                    &mut agent_candidates,
                    &mut agent_failures,
                    &mut removed_hooks,
                    &mut root_gitignore_removed,
                    &mut export_gitignore_removed,
                );
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
        let removed_agents = agent_candidates
            .difference(&agent_failures)
            .filter(|tool| !disabled_agents.contains(*tool))
            .cloned()
            .collect::<Vec<_>>();
        let removed_hooks = removed_hooks.into_iter().collect::<Vec<_>>();
        if let Err(err) = synrepo::registry::record_uninstall_progress(
            &project.path,
            &removed_agents,
            &removed_hooks,
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
    agent_candidates: &mut BTreeSet<String>,
    agent_failures: &mut BTreeSet<String>,
    removed_hooks: &mut BTreeSet<String>,
    root_gitignore_removed: &mut bool,
    export_gitignore_removed: &mut bool,
) {
    match action {
        UninstallAction::ProjectRemove {
            remove_action:
                RemoveAction::DeleteShim { tool, .. } | RemoveAction::StripMcpEntry { tool, .. },
            ..
        } => {
            if succeeded {
                agent_candidates.insert(tool.clone());
            } else {
                agent_failures.insert(tool.clone());
            }
        }
        UninstallAction::RemoveHook { name, .. } if succeeded => {
            removed_hooks.insert(name.clone());
        }
        UninstallAction::RemoveProjectGitignoreLine { entry, .. } if succeeded => {
            if entry == ".synrepo/" {
                *root_gitignore_removed = true;
            } else {
                *export_gitignore_removed = true;
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
        UninstallAction::RemoveHook { path, mode, .. } => remove_hook(path, mode),
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

fn remove_hook(path: &Path, mode: &str) -> anyhow::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read hook {}", path.display()))?;
    if mode == "full_file" && raw == full_hook_script() {
        std::fs::remove_file(path)
            .with_context(|| format!("failed to delete hook {}", path.display()))?;
        return Ok(());
    }
    let stripped = if raw.contains(HOOK_BEGIN) && raw.contains(HOOK_END) {
        strip_marked_hook(&raw)
    } else {
        strip_legacy_hook(&raw)
    };
    if stripped.trim().is_empty() {
        std::fs::remove_file(path)
            .with_context(|| format!("failed to delete empty hook {}", path.display()))?;
    } else {
        synrepo::util::atomic_write(path, stripped.as_bytes())
            .with_context(|| format!("failed to write hook {}", path.display()))?;
    }
    Ok(())
}

fn strip_marked_hook(raw: &str) -> String {
    let Some(begin) = raw.find(HOOK_BEGIN) else {
        return raw.to_string();
    };
    let Some(end_rel) = raw[begin..].find(HOOK_END) else {
        return raw.to_string();
    };
    let end = begin + end_rel + HOOK_END.len();
    let mut out = String::new();
    out.push_str(raw[..begin].trim_end());
    out.push('\n');
    out.push_str(raw[end..].trim_start());
    out
}

fn strip_legacy_hook(raw: &str) -> String {
    raw.lines()
        .filter(|line| {
            let trimmed = line.trim();
            trimmed != "# synrepo hook" && !trimmed.contains("synrepo reconcile --fast")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn push_failure(summary: &mut UninstallSummary, item: &PlannedAction, error: &str) {
    summary.applied.push(AppliedUninstallAction {
        action: item.action.clone(),
        succeeded: false,
        error: Some(error.to_string()),
    });
}
