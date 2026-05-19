use std::collections::BTreeSet;
use std::path::Path;

use synrepo::registry;
use synrepo::substrate::external_syntext::EXTERNAL_SYNTEXT_GITIGNORE_ENTRY;

use crate::cli_support::agent_shims::AgentTool;

use super::{ApplySummary, RemoveAction};

pub(super) fn record_registry_progress(
    repo_root: &Path,
    tool: Option<AgentTool>,
    summary: &ApplySummary,
) {
    let mut agent_candidates = BTreeSet::new();
    let mut agent_failures = BTreeSet::new();
    let mut removed_hooks = BTreeSet::new();
    let mut removed_agent_hooks = BTreeSet::new();
    let mut root_gitignore_removed = false;
    let mut syntext_gitignore_removed = false;
    let mut export_gitignore_removed = false;
    let mut synrepo_deleted = false;

    for item in &summary.applied {
        match &item.action {
            RemoveAction::DeleteShim { tool, .. } | RemoveAction::StripMcpEntry { tool, .. } => {
                if item.succeeded {
                    agent_candidates.insert(tool.clone());
                } else {
                    agent_failures.insert(tool.clone());
                }
            }
            RemoveAction::RemoveGitHook { name, .. } if item.succeeded => {
                removed_hooks.insert(name.clone());
            }
            RemoveAction::RemoveAgentHook { tool, .. } if item.succeeded => {
                removed_agent_hooks.insert(tool.clone());
            }
            RemoveAction::RemoveGitignoreLine { entry } if item.succeeded => {
                if entry == ".synrepo/" {
                    root_gitignore_removed = true;
                } else if entry == EXTERNAL_SYNTEXT_GITIGNORE_ENTRY {
                    syntext_gitignore_removed = true;
                } else {
                    export_gitignore_removed = true;
                }
            }
            RemoveAction::DeleteSynrepoDir if item.succeeded => {
                synrepo_deleted = true;
            }
            _ => {}
        }
    }

    if let Some(tool) = tool {
        let tool = tool.canonical_name().to_string();
        if agent_failures.contains(&tool) {
            agent_candidates.remove(&tool);
        }
    }
    let removed_agents = agent_candidates
        .difference(&agent_failures)
        .cloned()
        .collect::<Vec<_>>();
    let removed_hooks = removed_hooks.into_iter().collect::<Vec<_>>();
    let removed_agent_hooks = removed_agent_hooks.into_iter().collect::<Vec<_>>();
    if let Err(err) = registry::record_uninstall_progress(
        repo_root,
        registry::UninstallProgress {
            agent_tools: &removed_agents,
            hook_names: &removed_hooks,
            agent_hook_tools: &removed_agent_hooks,
            root_gitignore_removed,
            syntext_gitignore_removed,
            export_gitignore_removed,
            project_data_deleted: synrepo_deleted,
        },
    ) {
        tracing::warn!(error = %err, "registry update skipped after remove progress");
    }
}
