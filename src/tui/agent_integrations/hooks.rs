use std::path::{Path, PathBuf};

use serde_json::Value;

use super::{HookInstallStatus, HookStatus};

pub(super) fn resolve_hooks(repo_root: &Path, tool: &str) -> HookInstallStatus {
    let Some((path, client)) = hook_target(repo_root, tool) else {
        return HookInstallStatus {
            status: HookStatus::Unsupported,
            path: None,
            source: "unsupported".to_string(),
        };
    };
    let status = if hooks_installed(&path, client) {
        HookStatus::Installed
    } else {
        HookStatus::Missing
    };
    HookInstallStatus {
        status,
        path: Some(path),
        source: if status == HookStatus::Installed {
            "hook config".to_string()
        } else {
            "optional".to_string()
        },
    }
}

fn hook_target(repo_root: &Path, tool: &str) -> Option<(PathBuf, &'static str)> {
    match tool {
        "codex" => Some((repo_root.join(".codex").join("hooks.json"), "codex")),
        "claude" => Some((
            repo_root.join(".claude").join("settings.local.json"),
            "claude",
        )),
        _ => None,
    }
}

fn hooks_installed(path: &Path, client: &str) -> bool {
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<Value>(&text) else {
        return false;
    };
    let command_prompt =
        format!("synrepo agent-hook nudge --client {client} --event UserPromptSubmit");
    let command_pretool = format!("synrepo agent-hook nudge --client {client} --event PreToolUse");
    command_present(&value, "UserPromptSubmit", &command_prompt)
        && command_present(&value, "PreToolUse", &command_pretool)
}

fn command_present(value: &Value, event: &str, command: &str) -> bool {
    value
        .get("hooks")
        .and_then(|hooks| hooks.get(event))
        .and_then(Value::as_array)
        .map(|groups| {
            groups.iter().any(|group| {
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
            })
        })
        .unwrap_or(false)
}
