use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context};
use serde_json::{json, Value};
use synrepo::pipeline::writer::now_rfc3339;
use synrepo::registry::{self, AgentHookEntry};

use crate::cli_support::agent_shims::AgentTool;
use crate::cli_support::commands::setup::{load_json_config, write_json_config, StepOutcome};

const CODEX_FEATURE_MESSAGE: &str =
    "Codex hooks require `[features] codex_hooks = true` in Codex config.";

pub(crate) fn install_agent_hooks(
    repo_root: &Path,
    tool: AgentTool,
) -> anyhow::Result<StepOutcome> {
    let Some(target) = agent_hook_target(repo_root, tool) else {
        anyhow::bail!(
            "{} does not support synrepo agent nudge hooks",
            tool.display_name()
        );
    };

    let mut config = load_json_config(&target.path)?;
    let changed = merge_hook_config(&mut config, target.client, target.matcher)
        .with_context(|| format!("failed to merge {}", target.path.display()))?;
    if !changed {
        println!(
            "  synrepo nudge hooks already installed for {}.",
            tool.display_name()
        );
        if matches!(tool, AgentTool::Codex) {
            println!("  {CODEX_FEATURE_MESSAGE}");
        }
        record_agent_hook_best_effort(repo_root, tool, &target.path);
        return Ok(StepOutcome::AlreadyCurrent);
    }

    if let Some(parent) = target.path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    write_json_config(&target.path, &config)?;
    println!(
        "  Installed synrepo nudge hooks for {}: {}",
        tool.display_name(),
        display_path(&target.path, repo_root)
    );
    if matches!(tool, AgentTool::Codex) {
        println!("  {CODEX_FEATURE_MESSAGE}");
    }
    record_agent_hook_best_effort(repo_root, tool, &target.path);
    Ok(StepOutcome::Applied)
}

pub(crate) struct AgentHookTarget {
    pub(crate) path: PathBuf,
    pub(crate) client: &'static str,
    pub(crate) matcher: &'static str,
}

pub(crate) fn agent_hook_target(repo_root: &Path, tool: AgentTool) -> Option<AgentHookTarget> {
    match tool {
        AgentTool::Codex => Some(AgentHookTarget {
            path: repo_root.join(".codex/hooks.json"),
            client: "codex",
            matcher: "Bash|apply_patch|mcp__.*",
        }),
        AgentTool::Claude => Some(AgentHookTarget {
            path: repo_root.join(".claude/settings.local.json"),
            client: "claude",
            matcher: "Read|Grep|Glob|Edit|Write|Bash|mcp__.*",
        }),
        _ => None,
    }
}

pub(crate) fn agent_hook_commands_for_tool(tool: AgentTool) -> Option<[String; 2]> {
    let client = match tool {
        AgentTool::Codex => "codex",
        AgentTool::Claude => "claude",
        _ => return None,
    };
    Some(agent_hook_commands(client))
}

pub(crate) fn agent_hook_commands(client: &str) -> [String; 2] {
    [
        format!("synrepo agent-hook nudge --client {client} --event UserPromptSubmit"),
        format!("synrepo agent-hook nudge --client {client} --event PreToolUse"),
    ]
}

fn display_path(path: &Path, repo_root: &Path) -> String {
    path.strip_prefix(repo_root)
        .map(PathBuf::from)
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string()
}

pub(crate) fn merge_hook_config(
    config: &mut Value,
    client: &str,
    pre_tool_matcher: &str,
) -> anyhow::Result<bool> {
    let [command_prompt, command_pretool] = agent_hook_commands(client);

    let root = config
        .as_object_mut()
        .ok_or_else(|| anyhow!("hook config root must be a JSON object"))?;
    let hooks = root.entry("hooks").or_insert_with(|| json!({}));
    let hooks = hooks
        .as_object_mut()
        .ok_or_else(|| anyhow!("hooks must be a JSON object"))?;

    let mut changed = false;
    changed |= add_event_hook(hooks, "UserPromptSubmit", None, &command_prompt)?;
    changed |= add_event_hook(
        hooks,
        "PreToolUse",
        Some(pre_tool_matcher),
        &command_pretool,
    )?;
    Ok(changed)
}

fn record_agent_hook_best_effort(repo_root: &Path, tool: AgentTool, path: &Path) {
    let entry = AgentHookEntry {
        tool: tool.canonical_name().to_string(),
        path: registry_path_string(repo_root, path),
        installed_at: now_rfc3339(),
    };
    if let Err(err) = registry::record_agent_hooks(repo_root, vec![entry]) {
        tracing::warn!(
            error = %err,
            tool = tool.canonical_name(),
            "install registry update skipped after {} agent hook install",
            tool.display_name()
        );
    }
}

fn registry_path_string(repo_root: &Path, path: &Path) -> String {
    match path.strip_prefix(repo_root).map(PathBuf::from) {
        Ok(rel) => rel.to_string_lossy().into_owned(),
        Err(_) => path.to_string_lossy().into_owned(),
    }
}

fn add_event_hook(
    hooks: &mut serde_json::Map<String, Value>,
    event: &str,
    matcher: Option<&str>,
    command: &str,
) -> anyhow::Result<bool> {
    let groups = hooks
        .entry(event)
        .or_insert_with(|| Value::Array(Vec::new()));
    let groups = groups
        .as_array_mut()
        .ok_or_else(|| anyhow!("{event} hooks must be an array"))?;
    if groups
        .iter()
        .any(|group| group_contains_command(group, command))
    {
        return Ok(false);
    }

    let mut group = json!({
        "hooks": [{
            "type": "command",
            "command": command,
            "timeout": 5,
            "statusMessage": "Checking synrepo context guidance"
        }]
    });
    if let Some(matcher) = matcher {
        group["matcher"] = Value::String(matcher.to_string());
    }
    groups.push(group);
    Ok(true)
}

fn group_contains_command(group: &Value, command: &str) -> bool {
    group
        .get("hooks")
        .and_then(Value::as_array)
        .map(|hooks| {
            hooks.iter().any(|hook| {
                hook.get("command")
                    .and_then(Value::as_str)
                    .is_some_and(|existing| existing == command)
            })
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn merge_preserves_existing_hooks_and_adds_without_duplicates() {
        let mut value = json!({
            "permissions": { "allow": ["Bash(git diff:*)"] },
            "hooks": {
                "PreToolUse": [{
                    "matcher": "Bash",
                    "hooks": [{ "type": "command", "command": "echo existing" }]
                }]
            }
        });

        assert!(merge_hook_config(&mut value, "claude", "Read|Bash").unwrap());
        assert!(!merge_hook_config(&mut value, "claude", "Read|Bash").unwrap());

        assert_eq!(value["permissions"]["allow"][0], "Bash(git diff:*)");
        let pre = value["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(pre.len(), 2, "existing group plus synrepo group");
        let prompt = value["hooks"]["UserPromptSubmit"].as_array().unwrap();
        assert_eq!(prompt.len(), 1);
    }

    #[test]
    fn installer_writes_client_local_paths() {
        let _lock =
            synrepo::test_support::global_test_lock(synrepo::config::test_home::HOME_ENV_TEST_LOCK);
        let home = tempdir().unwrap();
        let _guard = synrepo::config::test_home::HomeEnvGuard::redirect_to(home.path());
        let repo = tempdir().unwrap();
        install_agent_hooks(repo.path(), AgentTool::Codex).unwrap();
        install_agent_hooks(repo.path(), AgentTool::Claude).unwrap();

        assert!(repo.path().join(".codex/hooks.json").exists());
        assert!(repo.path().join(".claude/settings.local.json").exists());
        assert_eq!(
            install_agent_hooks(repo.path(), AgentTool::Codex).unwrap(),
            StepOutcome::AlreadyCurrent
        );
        let entry = synrepo::registry::get(repo.path()).unwrap().unwrap();
        assert_eq!(entry.agent_hooks.len(), 2);
    }

    #[test]
    fn malformed_hooks_shape_errors_without_rewrite() {
        let mut value = json!({ "hooks": [] });
        let err = merge_hook_config(&mut value, "codex", "Bash").unwrap_err();
        assert!(err.to_string().contains("hooks must be a JSON object"));
    }
}
