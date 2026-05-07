use serde_json::json;
use synrepo::surface::task_route::{TaskRoute, SIGNAL_DETERMINISTIC_EDIT_CANDIDATE};

use super::{HookClient, HookEvent};

const NUDGE: &str = "synrepo hint: use graph context for repo questions, reviews, search, and edits. Read full source only when compact context is insufficient.";

pub(super) fn render_nudge(
    client: HookClient,
    event: HookEvent,
    route: Option<&TaskRoute>,
) -> String {
    let message = render_message(route);
    let value = match (client, event) {
        (HookClient::Codex, _) => json!({
            "systemMessage": message
        }),
        (_, HookEvent::UserPromptSubmit) => json!({
            "hookSpecificOutput": {
                "hookEventName": "UserPromptSubmit",
                "additionalContext": message
            }
        }),
        (HookClient::Claude, HookEvent::PreToolUse) => json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "additionalContext": message
            }
        }),
    };
    serde_json::to_string_pretty(&value).expect("nudge JSON should serialize")
}

fn render_message(route: Option<&TaskRoute>) -> String {
    let Some(route) = route.filter(|route| !route.signals.is_empty()) else {
        return NUDGE.to_string();
    };

    let mut message = NUDGE.to_string();
    message.push_str("\n\nRoute: ");
    message.push_str(&route.intent);
    message.push_str(" (budget=");
    message.push_str(&route.budget_tier);
    message.push_str(", llm_required=");
    message.push_str(if route.llm_required { "true" } else { "false" });
    message.push_str(").");
    message.push_str("\nSignals:");
    for signal in &route.signals {
        message.push('\n');
        if signal == SIGNAL_DETERMINISTIC_EDIT_CANDIDATE {
            if let Some(candidate) = &route.edit_candidate {
                message.push_str(signal);
                message.push_str(" Intent: ");
                message.push_str(&candidate.intent);
                continue;
            }
        }
        message.push_str(signal);
    }
    message.push_str("\nRecommended tools: ");
    message.push_str(&route.recommended_tools.join(", "));
    message
}
