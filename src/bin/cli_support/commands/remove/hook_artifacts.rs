//! Shared planning and removal helpers for Git hooks and agent nudge hooks.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context};
use serde_json::Value;
use synrepo::registry::{HookEntry, ProjectEntry};

use crate::cli_support::agent_shims::AgentTool;
use crate::cli_support::commands::agent_hooks::{agent_hook_commands_for_tool, agent_hook_target};
use crate::cli_support::commands::hooks::{full_hook_script, HOOK_BEGIN, HOOK_END, HOOK_NAMES};
use crate::cli_support::commands::setup::{load_json_config, write_json_config};

use super::{RemoveAction, RemovePlan};

pub(super) fn has_agent_hook(
    repo_root: &Path,
    tool: AgentTool,
    project: Option<&ProjectEntry>,
) -> bool {
    project
        .and_then(|entry| {
            entry
                .agent_hooks
                .iter()
                .find(|hook| hook.tool == tool.canonical_name())
        })
        .map(|hook| registry_path(repo_root, &hook.path).exists())
        .unwrap_or(false)
        || agent_hook_target(repo_root, tool)
            .map(|target| agent_hook_config_has_synrepo(tool, &target.path).unwrap_or(false))
            .unwrap_or(false)
}

pub(super) fn add_agent_hook_action(
    repo_root: &Path,
    tool: AgentTool,
    project: Option<&ProjectEntry>,
    plan: &mut RemovePlan,
) {
    let registry_entry = project.and_then(|entry| {
        entry
            .agent_hooks
            .iter()
            .find(|hook| hook.tool == tool.canonical_name())
    });
    let path = registry_entry
        .map(|hook| registry_path(repo_root, &hook.path))
        .or_else(|| agent_hook_target(repo_root, tool).map(|target| target.path));
    let Some(path) = path else {
        return;
    };
    if !path.exists() {
        return;
    }
    if registry_entry.is_none() && !agent_hook_config_has_synrepo(tool, &path).unwrap_or(false) {
        return;
    }
    if !plan.actions.iter().any(|action| {
        matches!(action, RemoveAction::RemoveAgentHook { tool: t, path: p } if t == tool.canonical_name() && p == &path)
    }) {
        plan.actions.push(RemoveAction::RemoveAgentHook {
            tool: tool.canonical_name().to_string(),
            path,
        });
    }
}

pub(super) fn add_git_hook_actions(
    repo_root: &Path,
    project: Option<&ProjectEntry>,
    plan: &mut RemovePlan,
) {
    let mut seen = BTreeSet::new();
    if let Some(project) = project {
        for hook in &project.hooks {
            add_git_hook_action(repo_root, hook, &mut seen, plan);
        }
    }
    for hook in scan_project_git_hooks(repo_root) {
        add_git_hook_action(repo_root, &hook, &mut seen, plan);
    }
}

fn add_git_hook_action(
    repo_root: &Path,
    hook: &HookEntry,
    seen: &mut BTreeSet<PathBuf>,
    plan: &mut RemovePlan,
) {
    let path = registry_path(repo_root, &hook.path);
    if !path.exists() || !seen.insert(path.clone()) {
        return;
    }
    plan.actions.push(RemoveAction::RemoveGitHook {
        name: hook.name.clone(),
        path,
        mode: hook.mode.clone(),
    });
}

fn scan_project_git_hooks(project: &Path) -> Vec<HookEntry> {
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
                mode: detected_hook_mode(&content).to_string(),
                installed_at: String::new(),
            })
        })
        .collect()
}

fn detected_hook_mode(content: &str) -> &'static str {
    if content == full_hook_script() {
        "full_file"
    } else if content.contains(HOOK_BEGIN) && content.contains(HOOK_END) {
        "marked_block"
    } else {
        "legacy"
    }
}

pub(crate) fn remove_git_hook(path: &Path, mode: &str) -> anyhow::Result<()> {
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

pub(crate) fn remove_agent_hook(tool: &str, path: &Path) -> anyhow::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let Some(tool) = agent_tool(tool) else {
        return Ok(());
    };
    let Some(commands) = agent_hook_commands_for_tool(tool) else {
        return Ok(());
    };
    let mut config = load_json_config(path)?;
    if strip_agent_hook_commands(&mut config, &commands)? {
        write_json_config(path, &config)?;
    }
    Ok(())
}

fn agent_hook_config_has_synrepo(tool: AgentTool, path: &Path) -> anyhow::Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    let Some(commands) = agent_hook_commands_for_tool(tool) else {
        return Ok(false);
    };
    let config = load_json_config(path)?;
    Ok(config_contains_agent_hook_commands(&config, &commands))
}

fn config_contains_agent_hook_commands(config: &Value, commands: &[String; 2]) -> bool {
    let Some(hooks) = config.get("hooks").and_then(Value::as_object) else {
        return false;
    };
    hooks.values().any(|groups| {
        groups.as_array().is_some_and(|groups| {
            groups.iter().any(|group| {
                group
                    .get("hooks")
                    .and_then(Value::as_array)
                    .is_some_and(|hooks| {
                        hooks
                            .iter()
                            .any(|hook| hook_command_matches(hook, commands))
                    })
            })
        })
    })
}

fn strip_agent_hook_commands(config: &mut Value, commands: &[String; 2]) -> anyhow::Result<bool> {
    let Some(root) = config.as_object_mut() else {
        return Ok(false);
    };
    let Some(hooks_value) = root.get_mut("hooks") else {
        return Ok(false);
    };
    let hooks = hooks_value
        .as_object_mut()
        .ok_or_else(|| anyhow!("hooks must be a JSON object"))?;
    let mut changed = false;
    for event in ["UserPromptSubmit", "PreToolUse"] {
        let mut remove_event = false;
        if let Some(groups_value) = hooks.get_mut(event) {
            let groups = groups_value
                .as_array_mut()
                .ok_or_else(|| anyhow!("{event} hooks must be an array"))?;
            let before = groups.len();
            for group in groups.iter_mut() {
                changed |= strip_group_commands(group, commands);
            }
            groups.retain(|group| {
                group
                    .get("hooks")
                    .and_then(Value::as_array)
                    .is_some_and(|hooks| !hooks.is_empty())
            });
            changed |= groups.len() != before;
            remove_event = groups.is_empty();
        }
        if remove_event {
            hooks.remove(event);
        }
    }
    if hooks.is_empty() {
        root.remove("hooks");
    }
    Ok(changed)
}

fn strip_group_commands(group: &mut Value, commands: &[String; 2]) -> bool {
    let Some(hooks) = group.get_mut("hooks").and_then(Value::as_array_mut) else {
        return false;
    };
    let before = hooks.len();
    hooks.retain(|hook| !hook_command_matches(hook, commands));
    hooks.len() != before
}

fn hook_command_matches(hook: &Value, commands: &[String; 2]) -> bool {
    hook.get("command")
        .and_then(Value::as_str)
        .is_some_and(|command| commands.iter().any(|expected| expected == command))
}

fn agent_tool(tool_name: &str) -> Option<AgentTool> {
    <AgentTool as clap::ValueEnum>::from_str(tool_name, false).ok()
}

pub(crate) fn registry_path(repo_root: &Path, stored: &str) -> PathBuf {
    let path = PathBuf::from(stored);
    if path.is_absolute() {
        path
    } else {
        repo_root.join(path)
    }
}
