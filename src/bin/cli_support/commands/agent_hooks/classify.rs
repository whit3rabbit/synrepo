use serde_json::Value;

use super::HookClient;

pub(super) fn prompt_needs_synrepo(prompt: &str) -> bool {
    let text = prompt.to_ascii_lowercase();
    let codebase_terms = [
        "codebase",
        "code base",
        "repository",
        "repo",
        "project",
        "architecture",
        "module",
        "symbol",
        "function",
        "cli",
        "entrypoint",
        "file",
        "files",
    ];
    let contextual_terms = [
        "search",
        "find",
        "where",
        "read",
        "tests",
        "test",
        "question",
        "questions",
        "how",
    ];
    let strong_workflow_terms = [
        "review",
        "audit",
        "grep",
        "trace",
        "call path",
        "impact",
        "risk",
        "edit",
        "change",
        "implement",
        "fix",
        "refactor",
    ];
    has_any(&text, &strong_workflow_terms)
        || (has_any(&text, &codebase_terms) && has_any(&text, &contextual_terms))
}

pub(super) fn tool_needs_synrepo(client: HookClient, input: &Value) -> bool {
    let Some(tool_name) = input.get("tool_name").and_then(Value::as_str) else {
        return false;
    };
    if tool_name.starts_with("mcp__synrepo__") {
        return false;
    }
    match client {
        HookClient::Claude => match tool_name {
            "Read" | "Grep" | "Glob" | "Edit" | "Write" => true,
            "Bash" => command_needs_synrepo(input),
            _ => false,
        },
        HookClient::Codex => match tool_name {
            "apply_patch" => true,
            "Bash" => command_needs_synrepo(input),
            _ => false,
        },
    }
}

pub(super) fn command_needs_synrepo(input: &Value) -> bool {
    let Some(command) = extract_command(input) else {
        return false;
    };
    shell_command_needs_synrepo(command)
}

pub(super) fn extract_command(input: &Value) -> Option<&str> {
    input
        .get("tool_input")
        .and_then(|value| value.get("command"))
        .and_then(Value::as_str)
        .or_else(|| input.get("command").and_then(Value::as_str))
}

pub(super) fn strip_rtk_prefix(command: &str) -> &str {
    let trimmed = command.trim_start();
    let Some(rest) = trimmed.strip_prefix("rtk") else {
        return trimmed;
    };
    let rest = rest.trim_start();
    rest.strip_prefix("proxy").unwrap_or(rest).trim_start()
}

fn shell_command_needs_synrepo(command: &str) -> bool {
    let command = strip_rtk_prefix(command);
    let mut words = command.split_whitespace();
    let first = words.next().unwrap_or("");
    let second = words.next().unwrap_or("");
    match first {
        "st" | "rg" | "grep" | "find" | "fd" => true,
        "sed" | "cat" | "head" | "tail" | "less" | "bat" | "nl" => true,
        "git" if matches!(second, "diff" | "show" | "grep" | "log") => true,
        "gh" if matches!(second, "pr" | "search") => true,
        _ => false,
    }
}

fn has_any(text: &str, terms: &[&str]) -> bool {
    terms.iter().any(|term| text.contains(term))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn prompt_classifier_matches_codebase_workflows() {
        for prompt in [
            "answer a question about this codebase",
            "review src/lib.rs",
            "trace where handle_query is called",
            "what tests cover this file?",
            "find the CLI entrypoint",
        ] {
            assert!(prompt_needs_synrepo(prompt), "{prompt}");
        }
    }

    #[test]
    fn prompt_classifier_skips_small_talk() {
        assert!(!prompt_needs_synrepo("what is the capital of France?"));
        assert!(!prompt_needs_synrepo("where should we eat tonight?"));
        assert!(!prompt_needs_synrepo("thanks"));
    }

    #[test]
    fn tool_classifier_strips_rtk_and_matches_search() {
        let input = json!({
            "tool_name": "Bash",
            "tool_input": { "command": "rtk proxy git diff -- src/lib.rs" }
        });
        assert!(tool_needs_synrepo(HookClient::Codex, &input));
        assert_eq!(strip_rtk_prefix("  rtk st -n foo src"), "st -n foo src");
        assert_eq!(strip_rtk_prefix("rtk proxy rg foo"), "rg foo");
    }

    #[test]
    fn tool_classifier_skips_tests_and_synrepo_mcp() {
        let test = json!({
            "tool_name": "Bash",
            "tool_input": { "command": "rtk cargo test" }
        });
        let synrepo = json!({ "tool_name": "mcp__synrepo__synrepo_card", "tool_input": {} });
        assert!(!tool_needs_synrepo(HookClient::Codex, &test));
        assert!(!tool_needs_synrepo(HookClient::Claude, &synrepo));
    }

    #[test]
    fn direct_read_and_patch_tools_match() {
        assert!(tool_needs_synrepo(
            HookClient::Claude,
            &json!({"tool_name": "Read", "tool_input": {}})
        ));
        assert!(tool_needs_synrepo(
            HookClient::Codex,
            &json!({"tool_name": "apply_patch", "tool_input": {}})
        ));
    }
}
