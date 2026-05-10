use serde_json::json;
use synrepo::surface::task_route::{TaskRoute, SIGNAL_DETERMINISTIC_EDIT_CANDIDATE};

use super::{HookClient, HookEvent};

const NUDGE: &str =
    "synrepo hint: broad -> synrepo_ask; exact -> synrepo_search; read source after compact context.";

pub(super) fn render_nudge(
    client: HookClient,
    event: HookEvent,
    route: Option<&TaskRoute>,
    existing_explain_available: bool,
) -> String {
    let message = render_message(route, existing_explain_available);
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

pub(super) fn route_prefers_existing_explain(route: &TaskRoute) -> bool {
    matches!(
        route.intent.as_str(),
        "risk-review" | "broad-context-question"
    )
}

fn render_message(route: Option<&TaskRoute>, existing_explain_available: bool) -> String {
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
    if existing_explain_available && route_prefers_existing_explain(route) {
        message.push_str("\nExisting explain: use synrepo_explain budget=deep for 1-3 focal targets; use synrepo_docs_search for design/why questions. Do not refresh from hooks.");
    }
    message
}

#[cfg(test)]
mod tests {
    use synrepo::surface::task_route::classify_task_route;

    use super::*;

    #[test]
    fn high_level_routes_include_existing_explain_when_available() {
        for prompt in [
            "review this module for regressions",
            "design the parser architecture",
            "refactor the sync pipeline",
            "security review auth flow",
        ] {
            let route = classify_task_route(prompt, None);
            let output = render_nudge(
                HookClient::Codex,
                HookEvent::UserPromptSubmit,
                Some(&route),
                true,
            );
            assert!(output.contains("synrepo_explain budget=deep"), "{output}");
            assert!(output.contains("synrepo_docs_search"), "{output}");
            assert!(!output.contains("synrepo_refresh_commentary"), "{output}");
        }
    }

    #[test]
    fn exact_search_routes_do_not_include_existing_explain_hint() {
        let route = classify_task_route("find Error::Other(anyhow", None);
        let output = render_nudge(
            HookClient::Codex,
            HookEvent::UserPromptSubmit,
            Some(&route),
            true,
        );

        assert!(!output.contains("Existing explain:"), "{output}");
    }
}
