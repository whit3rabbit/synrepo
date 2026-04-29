//! Plan builder for `synrepo remove`.
//!
//! Combines the registry entry for the current project with a filesystem scan
//! so pre-existing installs (no registry record) still produce a complete plan.
//! Per-agent scope is narrower; bulk scope is the union plus the root-gitignore
//! and `.synrepo/` lines.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use synrepo::config::Config;
use synrepo::registry::{self, AgentEntry, ProjectEntry};

use crate::cli_support::agent_shims::AgentTool;
use crate::cli_support::commands::mcp_config_has_synrepo;

use super::{all_agent_tools, RemoveAction, RemovePlan};

/// Build a [`RemovePlan`] by combining the registry entry for this project
/// with a filesystem scan that catches legacy installs the registry never saw.
pub(crate) fn build_plan(
    repo_root: &Path,
    tool: Option<AgentTool>,
    keep_synrepo_dir: bool,
) -> anyhow::Result<RemovePlan> {
    let project = registry::get(repo_root).unwrap_or(None);

    let mut plan = RemovePlan::default();
    let mut seen_mcp_paths: BTreeMap<PathBuf, String> = BTreeMap::new();
    let mut mcp_check_cache: BTreeMap<PathBuf, bool> = BTreeMap::new();

    match tool {
        Some(single) => {
            add_agent_actions(
                repo_root,
                single,
                project.as_ref(),
                &mut plan,
                &mut seen_mcp_paths,
                &mut mcp_check_cache,
            );
        }
        None => {
            for &t in all_agent_tools() {
                let has_registry_entry = project
                    .as_ref()
                    .map(|p| p.agents.iter().any(|a| a.tool == t.canonical_name()))
                    .unwrap_or(false);
                let has_shim = t.output_path(repo_root).exists();
                if !has_registry_entry && !has_shim {
                    continue;
                }
                add_agent_actions(
                    repo_root,
                    t,
                    project.as_ref(),
                    &mut plan,
                    &mut seen_mcp_paths,
                    &mut mcp_check_cache,
                );
            }

            // Catch lingering synrepo entries in MCP configs the per-agent loop
            // skipped (shim deleted but MCP entry left, or pre-existing installs
            // with no registry record).
            for &t in all_agent_tools() {
                let Some(rel) = t.mcp_config_relative_path() else {
                    continue;
                };
                let abs = repo_root.join(rel);
                if seen_mcp_paths.contains_key(&abs) {
                    continue;
                }
                if cached_has_synrepo(&abs, &mut mcp_check_cache) {
                    plan.actions.push(RemoveAction::StripMcpEntry {
                        tool: t.canonical_name().to_string(),
                        path: abs.clone(),
                    });
                    seen_mcp_paths.insert(abs, t.canonical_name().to_string());
                }
            }

            // Preserve `.bak` sidecars in the dry-run output.
            if let Some(project) = project.as_ref() {
                for agent in &project.agents {
                    if let Some(bak) = &agent.mcp_backup_path {
                        let abs = repo_root.join(bak);
                        if abs.exists() && !plan.preserved.contains(&abs) {
                            plan.preserved.push(abs);
                        }
                    }
                }
            }
            for &t in all_agent_tools() {
                if let Some(rel) = t.mcp_config_relative_path() {
                    let bak = repo_root.join(format!("{rel}.bak"));
                    if bak.exists() && !plan.preserved.contains(&bak) {
                        plan.preserved.push(bak);
                    }
                }
            }

            // Gitignore line: only if we added it.
            if project
                .as_ref()
                .map(|p| p.root_gitignore_entry_added)
                .unwrap_or(false)
                && gitignore_contains_line(repo_root, ".synrepo/")
            {
                plan.actions.push(RemoveAction::RemoveGitignoreLine {
                    entry: ".synrepo/".to_string(),
                });
            }
            if project
                .as_ref()
                .map(|p| p.export_gitignore_entry_added)
                .unwrap_or(false)
                && gitignore_contains_line(repo_root, "synrepo-context/")
            {
                plan.actions.push(RemoveAction::RemoveGitignoreLine {
                    entry: "synrepo-context/".to_string(),
                });
            }

            // `.synrepo/` itself. The caller's `--keep-synrepo-dir` short-circuits.
            let synrepo_dir = Config::synrepo_dir(repo_root);
            if !keep_synrepo_dir && synrepo_dir.exists() {
                plan.actions.push(RemoveAction::DeleteSynrepoDir);
            }
        }
    }

    Ok(plan)
}

fn add_agent_actions(
    repo_root: &Path,
    tool: AgentTool,
    project: Option<&ProjectEntry>,
    plan: &mut RemovePlan,
    seen_mcp_paths: &mut BTreeMap<PathBuf, String>,
    mcp_check_cache: &mut BTreeMap<PathBuf, bool>,
) {
    let registry_entry: Option<&AgentEntry> =
        project.and_then(|p| p.agents.iter().find(|a| a.tool == tool.canonical_name()));

    // Prefer the registry's recorded shim path (survives canonical-path
    // changes), fall back to `output_path()`.
    let shim_abs = registry_entry
        .map(|e| registry_path(repo_root, &e.shim_path))
        .unwrap_or_else(|| tool.output_path(repo_root));
    if shim_abs.exists() {
        plan.actions.push(RemoveAction::DeleteShim {
            tool: tool.canonical_name().to_string(),
            path: shim_abs,
        });
    }

    let mcp_abs = registry_entry
        .and_then(|e| {
            e.mcp_config_path
                .as_ref()
                .map(|p| registry_path(repo_root, p))
        })
        .or_else(|| {
            tool.mcp_config_relative_path()
                .map(|rel| repo_root.join(rel))
        });
    if let Some(abs) = mcp_abs {
        if cached_has_synrepo(&abs, mcp_check_cache) && !seen_mcp_paths.contains_key(&abs) {
            plan.actions.push(RemoveAction::StripMcpEntry {
                tool: tool.canonical_name().to_string(),
                path: abs.clone(),
            });
            seen_mcp_paths.insert(abs, tool.canonical_name().to_string());
        }
    }

    if let Some(entry) = registry_entry {
        if let Some(bak) = &entry.mcp_backup_path {
            let abs = repo_root.join(bak);
            if abs.exists() && !plan.preserved.contains(&abs) {
                plan.preserved.push(abs);
            }
        }
    } else if let Some(rel) = tool.mcp_config_relative_path() {
        let bak = repo_root.join(format!("{rel}.bak"));
        if bak.exists() && !plan.preserved.contains(&bak) {
            plan.preserved.push(bak);
        }
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

/// Memoized wrapper around `mcp_config_has_synrepo`. Multiple agents can share
/// the same MCP config path; without the cache, the per-agent loop and the
/// fallback bulk scan both re-read and re-parse the same file.
fn cached_has_synrepo(path: &Path, cache: &mut BTreeMap<PathBuf, bool>) -> bool {
    if let Some(&hit) = cache.get(path) {
        return hit;
    }
    let result = mcp_config_has_synrepo(path).unwrap_or(false);
    cache.insert(path.to_path_buf(), result);
    result
}

fn gitignore_contains_line(repo_root: &Path, entry: &str) -> bool {
    let path = repo_root.join(".gitignore");
    let Ok(content) = fs::read_to_string(path) else {
        return false;
    };
    content.lines().any(|l| l.trim() == entry)
}
