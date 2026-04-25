//! Apply a [`RemovePlan`] to disk.
//!
//! Each action is best-effort: one failure records the error on that row but
//! lets the others proceed. A deleted shim is worth preserving even if a later
//! MCP edit fails. The summary records succeeded/failed per action so the
//! caller (CLI or TUI) can report accurately.

use std::fs;
use std::path::Path;

use anyhow::Context;
use serde_json::Value;
use synrepo::config::Config;
use toml_edit::DocumentMut;

use crate::cli_support::commands::setup::{load_json_config, write_json_config};

use super::{AppliedAction, ApplySummary, RemoveAction, RemovePlan};

pub(crate) fn apply_plan(repo_root: &Path, plan: &RemovePlan) -> anyhow::Result<ApplySummary> {
    let mut summary = ApplySummary::default();
    for action in &plan.actions {
        let result = match action {
            RemoveAction::DeleteShim { path, .. } => delete_shim(path),
            RemoveAction::StripMcpEntry { path, .. } => strip_mcp_entry(path),
            RemoveAction::RemoveGitignoreLine { entry } => {
                synrepo::bootstrap::remove_from_root_gitignore(repo_root, entry).map(|_| ())
            }
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
