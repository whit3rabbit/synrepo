//! Uninstall-wizard bridge for `synrepo remove`.
//!
//! The library-side wizard in `synrepo::tui::wizard::uninstall` speaks
//! [`UninstallActionKind`]. The bin-side executor in `remove::apply` speaks
//! [`RemoveAction`]. This module owns the two-way translation and the
//! wizard-driven apply path so `remove::mod` stays focused on the plain-text
//! / JSON entry point.

use std::path::Path;

use synrepo::config::Config;
use synrepo::tui::{UninstallActionKind, UninstallPlan};

use crate::cli_support::agent_shims::AgentTool;

use super::{finalize_remove, guard_watch_daemon, RemoveAction, RemovePlan};

/// Apply a plan the operator confirmed through the uninstall wizard. The
/// wizard already played the role of the opt-in checkbox, so this path skips
/// the text `.synrepo/` prompt but still honors the watch-daemon guard.
pub(super) fn apply_wizard_plan(
    repo_root: &Path,
    tool: Option<AgentTool>,
    json: bool,
    force: bool,
    plan: RemovePlan,
) -> anyhow::Result<()> {
    if plan.is_empty() {
        if !json {
            println!("Nothing to remove.");
        }
        return Ok(());
    }

    let synrepo_dir = Config::synrepo_dir(repo_root);
    let has_synrepo_action = plan
        .actions
        .iter()
        .any(|a| matches!(a, RemoveAction::DeleteSynrepoDir));
    guard_watch_daemon(
        &synrepo_dir,
        has_synrepo_action,
        force,
        /*wizard=*/ true,
    )?;

    finalize_remove(repo_root, tool, &plan, json)
}

/// Translate bin-side [`RemoveAction`]s into library-side
/// [`UninstallActionKind`]s for the wizard. The two enums are shape-identical;
/// only the variant names differ (`DeleteShim` vs `RemoveShim`,
/// `StripMcpEntry` vs `RemoveMcpEntry`).
pub(super) fn to_uninstall_kinds(
    repo_root: &Path,
    actions: &[RemoveAction],
) -> Vec<UninstallActionKind> {
    actions
        .iter()
        .map(|a| match a {
            RemoveAction::DeleteShim { tool, path } => UninstallActionKind::RemoveShim {
                tool: tool.clone(),
                path: path.clone(),
            },
            RemoveAction::StripMcpEntry { tool, path } => UninstallActionKind::RemoveMcpEntry {
                tool: tool.clone(),
                path: path.clone(),
            },
            RemoveAction::RemoveGitignoreLine { entry } => {
                UninstallActionKind::RemoveGitignoreLine {
                    entry: entry.clone(),
                }
            }
            RemoveAction::RemoveGitHook { name, path, mode } => UninstallActionKind::RemoveHook {
                project: repo_root.to_path_buf(),
                name: name.clone(),
                path: path.clone(),
                mode: mode.clone(),
            },
            RemoveAction::RemoveAgentHook { tool, path } => UninstallActionKind::RemoveAgentHook {
                tool: tool.clone(),
                path: path.clone(),
            },
            RemoveAction::DeleteSynrepoDir => UninstallActionKind::DeleteSynrepoDir,
        })
        .collect()
}

/// Inverse of [`to_uninstall_kinds`]. The wizard returns only the rows the
/// operator kept checked, so this yields the exact list to hand to
/// [`apply_plan`].
pub(super) fn from_uninstall_kinds(actions: Vec<UninstallActionKind>) -> Vec<RemoveAction> {
    actions
        .into_iter()
        .map(|k| match k {
            UninstallActionKind::RemoveShim { tool, path } => {
                RemoveAction::DeleteShim { tool, path }
            }
            UninstallActionKind::RemoveMcpEntry { tool, path } => {
                RemoveAction::StripMcpEntry { tool, path }
            }
            UninstallActionKind::RemoveGitignoreLine { entry } => {
                RemoveAction::RemoveGitignoreLine { entry }
            }
            UninstallActionKind::RemoveHook {
                name, path, mode, ..
            } => RemoveAction::RemoveGitHook { name, path, mode },
            UninstallActionKind::RemoveAgentHook { tool, path } => {
                RemoveAction::RemoveAgentHook { tool, path }
            }
            UninstallActionKind::DeleteSynrepoDir => RemoveAction::DeleteSynrepoDir,
            other => panic!("unsupported action returned to synrepo remove: {other:?}"),
        })
        .collect()
}

/// Build a [`RemovePlan`] from the wizard's [`UninstallPlan`], preserving the
/// original plan's `preserved` list.
pub(super) fn wizard_plan_to_remove_plan(
    uplan: UninstallPlan,
    preserved: Vec<std::path::PathBuf>,
) -> RemovePlan {
    RemovePlan {
        actions: from_uninstall_kinds(uplan.actions),
        preserved,
    }
}
