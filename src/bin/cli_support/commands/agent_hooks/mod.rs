//! Client-side hook nudges for supported agent integrations.

mod classify;
mod install;
mod render;

use std::io::Read;
use std::path::{Path, PathBuf};

use serde_json::Value;
use synrepo::pipeline::context_metrics;
use synrepo::store::overlay::SqliteOverlayStore;
use synrepo::surface::task_route::{classify_task_route, TaskRoute};

use crate::cli_support::agent_shims::AgentTool;

pub(crate) use install::{agent_hook_commands_for_tool, agent_hook_target, install_agent_hooks};

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
    let route = task_route_from_input(client, event, &input);
    let reason = match event {
        HookEvent::UserPromptSubmit => {
            let prompt = input.get("prompt")?.as_str()?;
            classify::prompt_needs_synrepo(prompt)
        }
        HookEvent::PreToolUse => classify::tool_needs_synrepo(client, &input),
    };
    let route_has_signals = route
        .as_ref()
        .is_some_and(|route| !route.signals.is_empty());
    if !reason && !route_has_signals {
        return None;
    }
    record_hook_route_best_effort(route.as_ref());
    let existing_explain_available = route.as_ref().is_some_and(|route| {
        render::route_prefers_existing_explain(route) && overlay_commentary_available()
    });
    Some(render::render_nudge(
        client,
        event,
        route.as_ref(),
        existing_explain_available,
    ))
}

pub(crate) fn agent_hooks_supported(tool: AgentTool) -> bool {
    matches!(tool, AgentTool::Codex | AgentTool::Claude)
}

fn task_route_from_input(client: HookClient, event: HookEvent, input: &Value) -> Option<TaskRoute> {
    match event {
        HookEvent::UserPromptSubmit => {
            let prompt = input.get("prompt")?.as_str()?;
            Some(classify_task_route(prompt, None))
        }
        HookEvent::PreToolUse => tool_route(client, input),
    }
}

fn tool_route(client: HookClient, input: &Value) -> Option<TaskRoute> {
    let tool_name = input.get("tool_name").and_then(Value::as_str)?;
    if tool_name.starts_with("mcp__synrepo__") {
        return None;
    }
    if tool_name == "Bash" {
        if !classify::command_needs_synrepo(input) {
            return None;
        }
        let command = classify::extract_command(input)?;
        return Some(classify_task_route(
            &format!("search repository with command: {command}"),
            None,
        ));
    }

    let path = extract_tool_path(input);
    let task = match (client, tool_name) {
        (HookClient::Claude, "Read" | "Grep" | "Glob") => "find or read repository file",
        (HookClient::Claude, "Edit" | "Write") => "edit repository file",
        (HookClient::Codex, "apply_patch") => "edit repository file",
        _ => return None,
    };
    Some(classify_task_route(task, path))
}

fn extract_tool_path(input: &Value) -> Option<&str> {
    let tool_input = input.get("tool_input");
    tool_input
        .and_then(|value| value.get("file_path"))
        .and_then(Value::as_str)
        .or_else(|| {
            tool_input
                .and_then(|value| value.get("path"))
                .and_then(Value::as_str)
        })
        .or_else(|| input.get("file_path").and_then(Value::as_str))
        .or_else(|| input.get("path").and_then(Value::as_str))
}

fn record_hook_route_best_effort(route: Option<&TaskRoute>) {
    let Some(route) = route.filter(|route| !route.signals.is_empty()) else {
        return;
    };
    let Some(synrepo_dir) = discover_synrepo_dir() else {
        return;
    };
    context_metrics::record_hook_route_emission_best_effort(&synrepo_dir, route);
}

fn discover_synrepo_dir() -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        let candidate = dir.join(".synrepo");
        if is_synrepo_dir(&candidate) {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}

fn is_synrepo_dir(path: &Path) -> bool {
    path.is_dir()
}

fn overlay_commentary_available() -> bool {
    discover_synrepo_dir()
        .and_then(|dir| SqliteOverlayStore::open_existing(&dir.join("overlay")).ok())
        .and_then(|store| store.commentary_count().ok())
        .is_some_and(|count| count > 0)
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
        assert!(parsed["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap()
            .contains("[SYNREPO_CONTEXT_FAST_PATH]"));
        assert!(parsed["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap()
            .contains("synrepo_ask"));
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
        assert!(parsed["systemMessage"]
            .as_str()
            .unwrap()
            .contains("[SYNREPO_CONTEXT_FAST_PATH]"));
        assert!(parsed["systemMessage"]
            .as_str()
            .unwrap()
            .contains("broad -> synrepo_ask"));
        assert!(
            parsed.get("hookSpecificOutput").is_none(),
            "Codex prompt hooks should use common output fields"
        );
    }

    #[test]
    fn exact_identifier_prompt_recommends_search() {
        let body = json!({"prompt": "find Error::Other(anyhow"}).to_string();
        let output = nudge_output(HookClient::Codex, HookEvent::UserPromptSubmit, &body)
            .expect("exact search prompt should nudge");
        let parsed: Value = serde_json::from_str(&output).unwrap();
        let message = parsed["systemMessage"].as_str().unwrap();

        assert!(message.contains("synrepo_search"), "{message}");
        assert!(message.contains("exact -> synrepo_search"));
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
        assert!(parsed["systemMessage"]
            .as_str()
            .unwrap()
            .contains("[SYNREPO_CONTEXT_FAST_PATH]"));
    }

    #[test]
    fn pretool_message_stays_concise() {
        let body = json!({
            "tool_name": "Bash",
            "tool_input": { "command": "rtk st -n nudge src" }
        })
        .to_string();
        let output = nudge_output(HookClient::Codex, HookEvent::PreToolUse, &body)
            .expect("search command should nudge");
        let parsed: Value = serde_json::from_str(&output).unwrap();
        let message = parsed["systemMessage"].as_str().unwrap();
        assert!(message.len() < 400, "{message}");
        assert!(message.contains("synrepo hint"));
        assert!(message.contains("Recommended tools:"));
    }

    #[test]
    fn codex_pretool_skips_test_commands() {
        let body = json!({
            "tool_name": "Bash",
            "tool_input": { "command": "rtk .venv/bin/python -m pytest -q" }
        })
        .to_string();
        assert!(nudge_output(HookClient::Codex, HookEvent::PreToolUse, &body).is_none());
    }

    #[test]
    fn pretool_skips_external_mcp_tools() {
        let body = json!({
            "tool_name": "mcp__tui__interact",
            "tool_input": { "action": "press_enter" }
        })
        .to_string();
        assert!(nudge_output(HookClient::Claude, HookEvent::PreToolUse, &body).is_none());
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
        assert!(parsed["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap()
            .contains("[SYNREPO_LLM_NOT_REQUIRED]"));
    }

    #[test]
    fn prompt_edit_candidate_emits_intent_signal() {
        let body = json!({"prompt": "convert var to const in src/app.ts"}).to_string();
        let output = nudge_output(HookClient::Codex, HookEvent::UserPromptSubmit, &body)
            .expect("edit prompt should nudge");
        let parsed: Value = serde_json::from_str(&output).unwrap();
        assert!(parsed["systemMessage"]
            .as_str()
            .unwrap()
            .contains("[SYNREPO_DETERMINISTIC_EDIT_CANDIDATE] Intent: var-to-const"));
    }
}
