//! Client-side hook nudges for supported agent integrations.

mod classify;
mod install;
mod render;

use std::io::Read;

use serde_json::Value;

use crate::cli_support::agent_shims::AgentTool;

pub(crate) use install::install_agent_hooks;

const MAX_STDIN_BYTES: usize = 256 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HookClient {
    Codex,
    Claude,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HookEvent {
    UserPromptSubmit,
    PreToolUse,
}

pub(crate) fn run_nudge(client: &str, event: &str) -> anyhow::Result<()> {
    let Some(output) = nudge_output_from_stdin(client, event)? else {
        return Ok(());
    };
    println!("{output}");
    Ok(())
}

fn nudge_output_from_stdin(client: &str, event: &str) -> anyhow::Result<Option<String>> {
    let client = match parse_client(client) {
        Some(client) => client,
        None => return Ok(None),
    };
    let event = match parse_event(event) {
        Some(event) => event,
        None => return Ok(None),
    };

    let mut stdin = std::io::stdin().lock().take(MAX_STDIN_BYTES as u64);
    let mut bytes = Vec::new();
    if stdin.read_to_end(&mut bytes).is_err() {
        return Ok(None);
    }
    let body = String::from_utf8_lossy(&bytes);
    Ok(nudge_output(client, event, &body))
}

fn parse_client(raw: &str) -> Option<HookClient> {
    match raw.to_ascii_lowercase().as_str() {
        "codex" => Some(HookClient::Codex),
        "claude" => Some(HookClient::Claude),
        _ => None,
    }
}

fn parse_event(raw: &str) -> Option<HookEvent> {
    match raw {
        "UserPromptSubmit" | "user-prompt-submit" | "user_prompt_submit" => {
            Some(HookEvent::UserPromptSubmit)
        }
        "PreToolUse" | "pre-tool-use" | "pre_tool_use" => Some(HookEvent::PreToolUse),
        _ => None,
    }
}

pub(crate) fn nudge_output(client: HookClient, event: HookEvent, body: &str) -> Option<String> {
    let input: Value = serde_json::from_str(body).ok()?;
    let reason = match event {
        HookEvent::UserPromptSubmit => {
            let prompt = input.get("prompt")?.as_str()?;
            classify::prompt_needs_synrepo(prompt)
        }
        HookEvent::PreToolUse => classify::tool_needs_synrepo(client, &input),
    };
    if !reason {
        return None;
    }
    Some(render::render_nudge(client, event))
}

pub(crate) fn agent_hooks_supported(tool: AgentTool) -> bool {
    matches!(tool, AgentTool::Codex | AgentTool::Claude)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn unsupported_client_or_event_exits_open() {
        assert!(nudge_output(HookClient::Codex, HookEvent::UserPromptSubmit, "{}").is_none());
        assert!(parse_client("cursor").is_none());
        assert!(parse_event("Stop").is_none());
    }

    #[test]
    fn user_prompt_review_gets_context_nudge() {
        let body = json!({"prompt": "Review these files for regressions"}).to_string();
        let output = nudge_output(HookClient::Claude, HookEvent::UserPromptSubmit, &body)
            .expect("review prompt should nudge");
        let parsed: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(
            parsed["hookSpecificOutput"]["hookEventName"],
            "UserPromptSubmit"
        );
        assert!(parsed["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap()
            .contains("synrepo"));
    }

    #[test]
    fn codex_user_prompt_uses_system_message_shape() {
        let body = json!({"prompt": "Review the repository layout"}).to_string();
        let output = nudge_output(HookClient::Codex, HookEvent::UserPromptSubmit, &body)
            .expect("review prompt should nudge");
        let parsed: Value = serde_json::from_str(&output).unwrap();
        assert!(parsed["systemMessage"]
            .as_str()
            .unwrap()
            .contains("synrepo"));
        assert!(
            parsed.get("hookSpecificOutput").is_none(),
            "Codex prompt hooks should use common output fields"
        );
    }

    #[test]
    fn irrelevant_prompt_has_no_output() {
        let body = json!({"prompt": "say hello"}).to_string();
        assert!(nudge_output(HookClient::Codex, HookEvent::UserPromptSubmit, &body).is_none());
    }

    #[test]
    fn codex_pretool_uses_system_message_shape() {
        let body = json!({
            "tool_name": "Bash",
            "tool_input": { "command": "rtk st -n nudge src" }
        })
        .to_string();
        let output = nudge_output(HookClient::Codex, HookEvent::PreToolUse, &body)
            .expect("search command should nudge");
        let parsed: Value = serde_json::from_str(&output).unwrap();
        assert!(parsed["systemMessage"]
            .as_str()
            .unwrap()
            .contains("synrepo"));
    }

    #[test]
    fn claude_pretool_uses_additional_context_shape() {
        let body = json!({
            "tool_name": "Read",
            "tool_input": { "file_path": "src/lib.rs" }
        })
        .to_string();
        let output = nudge_output(HookClient::Claude, HookEvent::PreToolUse, &body)
            .expect("Read should nudge");
        let parsed: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["hookSpecificOutput"]["hookEventName"], "PreToolUse");
        assert!(
            parsed["hookSpecificOutput"]["permissionDecision"].is_null(),
            "nudge must not allow or deny tools"
        );
    }
}
