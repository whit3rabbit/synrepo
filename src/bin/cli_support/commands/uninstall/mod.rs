//! `synrepo uninstall` guided full-product teardown.

mod apply;
mod binary;
mod plan;

use std::path::Path;

use synrepo::tui::{
    run_uninstall_wizard, stdout_is_tty, TuiOptions, UninstallActionKind, UninstallWizardOutcome,
};

use crate::cli_support::commands::remove::RemoveAction;

use apply::{apply_uninstall_plan, UninstallSummary};
pub(crate) use plan::build_uninstall_plan;
use plan::{PlannedAction, UninstallAction, UninstallPlan};

pub(crate) fn uninstall(
    repo_root: &Path,
    apply: bool,
    json: bool,
    force: bool,
    delete_data: bool,
    keep_binary: bool,
) -> anyhow::Result<()> {
    let plan = build_uninstall_plan(repo_root, delete_data, keep_binary)?;
    let rows = wizard_rows(&plan);

    let wizard = !json && !force && stdout_is_tty() && !rows.is_empty();
    if wizard {
        match run_uninstall_wizard(rows, plan.preserved.clone(), TuiOptions::default())? {
            UninstallWizardOutcome::NonTty => {}
            UninstallWizardOutcome::Cancelled => {
                println!("uninstall wizard cancelled; no changes applied.");
                return Ok(());
            }
            UninstallWizardOutcome::Completed { plan: selected } => {
                let selected_plan = plan_with_wizard_selection(plan, &selected.actions);
                let summary = apply_uninstall_plan(&selected_plan, force)?;
                render_summary(&summary, json)?;
                return Ok(());
            }
        }
    }

    if apply && !force {
        anyhow::bail!(
            "`synrepo uninstall --apply` requires `--force` outside the guided TTY wizard"
        );
    }

    if json {
        print!("{}", serde_json::to_string_pretty(&plan)?);
        println!();
    } else {
        render_plan(&plan);
    }

    if !apply {
        if !json {
            println!(
                "\nDry run. Run `synrepo uninstall --apply --force` to execute selected non-data actions."
            );
            println!("Add `--delete-data` to include .synrepo/ and ~/.synrepo data deletion.");
        }
        return Ok(());
    }

    let summary = apply_uninstall_plan(&plan, force)?;
    render_summary(&summary, json)
}

fn wizard_rows(plan: &UninstallPlan) -> Vec<UninstallActionKind> {
    plan.projects
        .iter()
        .flat_map(|project| project.actions.iter())
        .chain(plan.global.iter())
        .filter_map(|item| action_to_wizard_kind(&item.action))
        .collect()
}

fn plan_with_wizard_selection(
    mut plan: UninstallPlan,
    selected: &[UninstallActionKind],
) -> UninstallPlan {
    for item in plan.actions_mut() {
        item.enabled = action_to_wizard_kind(&item.action)
            .map(|kind| selected.contains(&kind))
            .unwrap_or(false);
    }
    plan
}

fn action_to_wizard_kind(action: &UninstallAction) -> Option<UninstallActionKind> {
    match action {
        UninstallAction::ProjectRemove {
            project,
            remove_action,
        } => match remove_action {
            RemoveAction::DeleteShim { tool, path } => Some(UninstallActionKind::RemoveShim {
                tool: tool.clone(),
                path: path.clone(),
            }),
            RemoveAction::StripMcpEntry { tool, path } => {
                Some(UninstallActionKind::RemoveMcpEntry {
                    tool: tool.clone(),
                    path: path.clone(),
                })
            }
            RemoveAction::RemoveGitignoreLine { entry } => {
                Some(UninstallActionKind::RemoveGitignoreLine {
                    entry: entry.clone(),
                })
            }
            RemoveAction::RemoveGitHook { name, path, mode } => {
                Some(UninstallActionKind::RemoveHook {
                    project: project.clone(),
                    name: name.clone(),
                    path: path.clone(),
                    mode: mode.clone(),
                })
            }
            RemoveAction::RemoveAgentHook { tool, path } => {
                Some(UninstallActionKind::RemoveAgentHook {
                    tool: tool.clone(),
                    path: path.clone(),
                })
            }
            RemoveAction::DeleteSynrepoDir => Some(UninstallActionKind::DeleteSynrepoDir),
        },
        UninstallAction::RemoveHook {
            project,
            name,
            path,
            mode,
            ..
        } => Some(UninstallActionKind::RemoveHook {
            project: project.clone(),
            name: name.clone(),
            path: path.clone(),
            mode: mode.clone(),
        }),
        UninstallAction::RemoveAgentHook { tool, path, .. } => {
            Some(UninstallActionKind::RemoveAgentHook {
                tool: tool.clone(),
                path: path.clone(),
            })
        }
        UninstallAction::DeleteProjectSynrepoDir { project, path } => {
            Some(UninstallActionKind::DeleteProjectSynrepoDir {
                project: project.clone(),
                path: path.clone(),
            })
        }
        UninstallAction::RemoveProjectGitignoreLine { project, entry } => {
            Some(UninstallActionKind::RemoveProjectGitignoreLine {
                project: project.clone(),
                entry: entry.clone(),
            })
        }
        UninstallAction::RemoveExportDir { project, path } => {
            Some(UninstallActionKind::RemoveExportDir {
                project: project.clone(),
                path: path.clone(),
            })
        }
        UninstallAction::DeleteGlobalSynrepoDir { path } => {
            Some(UninstallActionKind::DeleteGlobalSynrepoDir { path: path.clone() })
        }
        UninstallAction::DeleteBinary { path } => {
            Some(UninstallActionKind::DeleteBinary { path: path.clone() })
        }
    }
}

fn render_plan(plan: &UninstallPlan) {
    if plan.is_empty() {
        println!("No uninstallable synrepo artifacts found.");
    }
    for project in &plan.projects {
        println!("Project: {}", project.path.display());
        for item in &project.actions {
            println!("  {:<8} {}", state_label(item), action_label(&item.action));
        }
    }
    if !plan.global.is_empty() {
        println!("Global:");
        for item in &plan.global {
            println!("  {:<8} {}", state_label(item), action_label(&item.action));
        }
    }
    if !plan.preserved.is_empty() {
        println!("\nPreserved:");
        for path in &plan.preserved {
            println!("  {}", path.display());
        }
    }
    if !plan.manual_followups.is_empty() {
        println!("\nManual follow-up:");
        for command in &plan.manual_followups {
            println!("  {command}");
        }
    }
    if !plan.skipped.is_empty() {
        println!("\nSkipped:");
        for skipped in &plan.skipped {
            println!("  {} ({})", skipped.target, skipped.reason);
        }
    }
}

fn state_label(item: &PlannedAction) -> &'static str {
    if item.enabled {
        "remove"
    } else if item.destructive {
        "keep"
    } else {
        "skip"
    }
}

fn action_label(action: &UninstallAction) -> String {
    match action {
        UninstallAction::ProjectRemove { remove_action, .. } => match remove_action {
            RemoveAction::DeleteShim { tool, path } => {
                format!("delete {tool} instructions at {}", path.display())
            }
            RemoveAction::StripMcpEntry { tool, path } => {
                format!("strip {tool} MCP entry from {}", path.display())
            }
            RemoveAction::RemoveGitignoreLine { entry } => {
                format!("remove `{entry}` from .gitignore")
            }
            RemoveAction::RemoveGitHook { name, path, .. } => {
                format!("remove {name} Git hook at {}", path.display())
            }
            RemoveAction::RemoveAgentHook { tool, path } => {
                format!("remove {tool} agent nudge hooks from {}", path.display())
            }
            RemoveAction::DeleteSynrepoDir => "delete .synrepo/".to_string(),
        },
        UninstallAction::RemoveHook { name, path, .. } => {
            format!("remove {name} Git hook at {}", path.display())
        }
        UninstallAction::RemoveAgentHook { tool, path, .. } => {
            format!("remove {tool} agent nudge hooks from {}", path.display())
        }
        UninstallAction::DeleteProjectSynrepoDir { path, .. } => {
            format!("delete project data at {}", path.display())
        }
        UninstallAction::RemoveProjectGitignoreLine { entry, .. } => {
            format!("remove `{entry}` from .gitignore after deleting generated files")
        }
        UninstallAction::RemoveExportDir { path, .. } => {
            format!("delete generated export output at {}", path.display())
        }
        UninstallAction::DeleteGlobalSynrepoDir { path } => {
            format!("delete global synrepo state at {}", path.display())
        }
        UninstallAction::DeleteBinary { path } => {
            format!("delete synrepo binary at {}", path.display())
        }
    }
}

fn render_summary(summary: &UninstallSummary, json: bool) -> anyhow::Result<()> {
    if json {
        print!("{}", serde_json::to_string_pretty(summary)?);
        println!();
        return Ok(());
    }
    println!();
    for item in &summary.applied {
        let label = action_label(&item.action);
        if item.succeeded {
            println!("  ok: {label}");
        } else {
            println!(
                "  FAILED: {label} ({})",
                item.error.as_deref().unwrap_or("unknown error")
            );
        }
    }
    if !summary.manual_followups.is_empty() {
        println!("\nManual follow-up:");
        for command in &summary.manual_followups {
            println!("  {command}");
        }
    }
    if !summary.skipped.is_empty() {
        println!("\nSkipped:");
        for skipped in &summary.skipped {
            println!("  {} ({})", skipped.target, skipped.reason);
        }
    }
    println!("Uninstall flow complete.");
    Ok(())
}

#[cfg(test)]
mod tests;
