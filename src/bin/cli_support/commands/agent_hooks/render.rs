use serde_json::json;

use super::{HookClient, HookEvent};

const NUDGE: &str = "Synrepo nudge: this repo has .synrepo context. For codebase questions, file reviews, broad search, and pre-edit work, call synrepo_orient first, then use synrepo_search with output_mode=\"compact\" or synrepo_find, followed by synrepo_explain, synrepo_minimum_context, synrepo_risks, and synrepo_tests as needed. Full source reads are an escalation when cards are insufficient.";

pub(super) fn render_nudge(client: HookClient, event: HookEvent) -> String {
    let value = match (client, event) {
        (HookClient::Codex, _) => json!({
            "systemMessage": NUDGE
        }),
        (_, HookEvent::UserPromptSubmit) => json!({
            "hookSpecificOutput": {
                "hookEventName": "UserPromptSubmit",
                "additionalContext": NUDGE
            }
        }),
        (HookClient::Claude, HookEvent::PreToolUse) => json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "additionalContext": NUDGE
            }
        }),
    };
    serde_json::to_string_pretty(&value).expect("nudge JSON should serialize")
}
