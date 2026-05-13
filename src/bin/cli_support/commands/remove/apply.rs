//! Apply a [`RemovePlan`] to disk.
//!
//! Each action is best-effort: one failure records the error on that row but
//! lets the others proceed. A deleted shim is worth preserving even if a later
//! MCP edit fails. The summary records succeeded/failed per action so the
//! caller (CLI or TUI) can report accurately.

use std::fs;
use std::path::Path;

use agent_config::{AgentConfigError, Scope, UninstallReport};
use anyhow::Context;
use serde_json::Value;
use synrepo::config::Config;
use toml_edit::DocumentMut;

use crate::cli_support::agent_shims::{
    AgentTool, ShimPlacement, SYNREPO_INSTALL_NAME, SYNREPO_INSTALL_OWNER,
};
use crate::cli_support::commands::mcp_config_has_synrepo;
use crate::cli_support::commands::setup::{load_json_config, write_json_config};

use super::hook_artifacts::{remove_agent_hook, remove_git_hook};
use super::{AppliedAction, ApplySummary, RemoveAction, RemovePlan};

pub(crate) fn apply_plan(repo_root: &Path, plan: &RemovePlan) -> anyhow::Result<ApplySummary> {
    let mut summary = ApplySummary::default();
    for action in &plan.actions {
        let result = match action {
            RemoveAction::DeleteShim { tool, path } => uninstall_shim(repo_root, tool, path),
            RemoveAction::StripMcpEntry { tool, path } => {
                uninstall_mcp_entry(repo_root, tool, path)
            }
            RemoveAction::RemoveGitignoreLine { entry } => {
                synrepo::bootstrap::remove_from_root_gitignore(repo_root, entry).map(|_| ())
            }
            RemoveAction::RemoveGitHook { path, mode, .. } => remove_git_hook(path, mode),
            RemoveAction::RemoveAgentHook { tool, path } => remove_agent_hook(tool, path),
            RemoveAction::DeleteSynrepoDir => {
                let synrepo_dir = Config::synrepo_dir(repo_root);
                if synrepo_dir.exists() {
                    fs::remove_dir_all(&synrepo_dir)
                        .with_context(|| format!("failed to delete {}", synrepo_dir.display()))
                } else {
                    Ok(())
                }
            }
        };
        summary.applied.push(AppliedAction {
            action: action.clone(),
            succeeded: result.is_ok(),
            error: result.err().map(|e| e.to_string()),
        });
    }
    Ok(summary)
}

fn uninstall_shim(repo_root: &Path, tool_name: &str, path: &Path) -> anyhow::Result<()> {
    let Some(tool) = agent_tool(tool_name) else {
        return delete_unknown_legacy_shim(tool_name, path);
    };
    let scope = scope_for_path(repo_root, path);
    let result = match tool.placement_kind() {
        ShimPlacement::Skill { name } => {
            let Some(id) = tool.agent_config_id() else {
                return delete_legacy_shim(tool, path);
            };
            let Some(installer) = agent_config::skill_by_id(id) else {
                return delete_legacy_shim(tool, path);
            };
            installer
                .uninstall_skill(&scope, name, SYNREPO_INSTALL_OWNER)
                .map_err(anyhow::Error::new)
        }
        ShimPlacement::Instruction { name, .. } => {
            let Some(id) = tool.agent_config_id() else {
                return delete_legacy_shim(tool, path);
            };
            let Some(installer) = agent_config::instruction_by_id(id) else {
                return delete_legacy_shim(tool, path);
            };
            installer
                .uninstall_instruction(&scope, name, SYNREPO_INSTALL_OWNER)
                .map_err(anyhow::Error::new)
        }
        ShimPlacement::Local => return delete_legacy_shim(tool, path),
    };

    match result {
        Ok(report) if report_changed_or_gone(&report, path) => Ok(()),
        Ok(_) => {
            warn_legacy_remove(tool_name, path);
            delete_legacy_shim(tool, path)
        }
        Err(err) if is_unowned_agent_config_error(&err) => {
            warn_legacy_remove(tool_name, path);
            delete_legacy_shim(tool, path)
        }
        Err(err) => Err(err).with_context(|| {
            format!(
                "failed to uninstall {} shim through agent-config",
                tool.display_name()
            )
        }),
    }
}

fn delete_legacy_shim(tool: AgentTool, path: &Path) -> anyhow::Result<()> {
    if is_shared_instruction_host(tool, path) {
        return delete_shared_host_if_exact_legacy_shim(tool, path);
    }
    delete_shim(path)
}

fn delete_unknown_legacy_shim(tool_name: &str, path: &Path) -> anyhow::Result<()> {
    if path.file_name().and_then(|name| name.to_str()) == Some("AGENTS.md") {
        anyhow::bail!(
            "refusing to delete shared instruction file {} for unknown legacy tool {}",
            path.display(),
            tool_name
        );
    }
    delete_shim(path)
}

fn is_shared_instruction_host(tool: AgentTool, path: &Path) -> bool {
    matches!(tool, AgentTool::OpenCode)
        && path.file_name().and_then(|name| name.to_str()) == Some("AGENTS.md")
}

fn delete_shared_host_if_exact_legacy_shim(tool: AgentTool, path: &Path) -> anyhow::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let existing =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    if existing.trim_end_matches('\n') == tool.shim_content().trim_end_matches('\n') {
        return delete_shim(path);
    }
    anyhow::bail!(
        "refusing to delete shared instruction file {}; it is not the exact legacy synrepo {} shim",
        path.display(),
        tool.display_name()
    )
}

fn uninstall_mcp_entry(repo_root: &Path, tool_name: &str, path: &Path) -> anyhow::Result<()> {
    let Some(tool) = agent_tool(tool_name) else {
        return strip_mcp_entry(path);
    };
    let Some(id) = tool.agent_config_id() else {
        return strip_mcp_entry(path);
    };
    let Some(installer) = agent_config::mcp_by_id(id) else {
        return strip_mcp_entry(path);
    };
    let scope = scope_for_path(repo_root, path);
    match installer.uninstall_mcp(&scope, SYNREPO_INSTALL_NAME, SYNREPO_INSTALL_OWNER) {
        Ok(report) if report_changed_or_gone(&report, path) => Ok(()),
        Ok(_) => {
            if mcp_config_has_synrepo(path)? {
                warn_legacy_remove(tool_name, path);
                strip_mcp_entry(path)
            } else {
                Ok(())
            }
        }
        Err(err) if is_present_unowned(&err) && mcp_config_has_synrepo(path)? => {
            warn_legacy_remove(tool_name, path);
            strip_mcp_entry(path)
        }
        Err(err) => Err(anyhow::Error::new(err)).with_context(|| {
            format!(
                "failed to uninstall synrepo MCP entry for {} through agent-config",
                tool.display_name()
            )
        }),
    }
}

fn agent_tool(tool_name: &str) -> Option<AgentTool> {
    if tool_name == "opencode" {
        return Some(AgentTool::OpenCode);
    }
    <AgentTool as clap::ValueEnum>::from_str(tool_name, false).ok()
}

fn scope_for_path(repo_root: &Path, path: &Path) -> Scope {
    if path.starts_with(repo_root) {
        Scope::Local(repo_root.to_path_buf())
    } else {
        Scope::Global
    }
}

fn report_changed_or_gone(report: &UninstallReport, path: &Path) -> bool {
    !report.not_installed || !path.exists()
}

fn is_unowned_agent_config_error(err: &anyhow::Error) -> bool {
    err.downcast_ref::<AgentConfigError>()
        .map(is_present_unowned)
        .unwrap_or(false)
}

fn is_present_unowned(err: &AgentConfigError) -> bool {
    matches!(err, AgentConfigError::NotOwnedByCaller { actual: None, .. })
}

fn warn_legacy_remove(tool_name: &str, path: &Path) {
    eprintln!(
        "warning: removing legacy unowned synrepo install for {tool_name} at {}; \
         agent-config cannot prove this entry was installed by synrepo, so inspect it before relying on removal.",
        path.display()
    );
}

fn delete_shim(path: &Path) -> anyhow::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    fs::remove_file(path).with_context(|| format!("failed to delete {}", path.display()))?;

    // Clean up empty parent directories up to the repo root, but stop at the
    // first non-empty one. This removes `.claude/skills/synrepo/` leftovers
    // without ever touching `.claude/` itself if the user has other files
    // under it.
    let mut current = path.parent();
    while let Some(dir) = current {
        match fs::read_dir(dir) {
            Ok(mut entries) => {
                if entries.next().is_some() {
                    break;
                }
            }
            Err(_) => break,
        }
        if fs::remove_dir(dir).is_err() {
            break;
        }
        current = dir.parent();
    }
    Ok(())
}

/// Strip the `synrepo` key from either a JSON MCP config (`.mcp.json`,
/// `.cursor/mcp.json`, etc.) or from `.codex/config.toml`. Leaves every other
/// MCP entry untouched. If removing synrepo leaves `mcpServers`,
/// `mcp_servers`, or legacy `mcp` empty, we also drop that key, but we still
/// write the file (possibly `{}`) so user ownership is preserved.
fn strip_mcp_entry(path: &Path) -> anyhow::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    if path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e == "toml")
        .unwrap_or(false)
    {
        return strip_mcp_entry_toml(path);
    }
    strip_mcp_entry_json(path)
}

fn strip_mcp_entry_json(path: &Path) -> anyhow::Result<()> {
    let mut config = load_json_config(path)?;
    if !config.is_object() {
        return Ok(()); // nothing we can edit; leave the file alone
    }
    let root = config.as_object_mut().expect("object checked above");

    // `.mcp.json`, `.cursor/mcp.json`, `.windsurf/mcp.json`, `.roo/mcp.json`
    // use `mcpServers.synrepo`. `opencode.json` uses `mcp.synrepo`. Handle
    // both by scrubbing either container, whichever is present.
    for container_key in ["mcpServers", "mcp"] {
        let Some(container) = root.get_mut(container_key).and_then(Value::as_object_mut) else {
            continue;
        };
        container.remove("synrepo");
        if container.is_empty() {
            root.remove(container_key);
        }
    }

    write_json_config(path, &config)
}

fn strip_mcp_entry_toml(path: &Path) -> anyhow::Result<()> {
    let raw =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut doc: DocumentMut = raw
        .parse()
        .with_context(|| format!("failed to parse {} as TOML", path.display()))?;
    for container_key in ["mcp_servers", "mcp"] {
        if let Some(container) = doc.get_mut(container_key).and_then(|i| i.as_table_mut()) {
            container.remove("synrepo");
            if container.is_empty() {
                doc.as_table_mut().remove(container_key);
            }
        }
    }
    synrepo::util::atomic_write(path, doc.to_string().as_bytes())
        .with_context(|| format!("failed to atomically write {}", path.display()))
}
