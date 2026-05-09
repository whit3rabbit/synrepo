use super::{EditCandidate, TaskRoute};

pub(super) fn route(
    intent: &str,
    confidence: f32,
    tools: &[&str],
    budget_tier: &str,
    llm_required: bool,
    reason: &str,
) -> TaskRoute {
    TaskRoute {
        intent: intent.to_string(),
        confidence,
        recommended_tools: tools.iter().map(|tool| (*tool).to_string()).collect(),
        budget_tier: budget_tier.to_string(),
        llm_required,
        edit_candidate: None,
        signals: Vec::new(),
        reason: reason.to_string(),
        routing_strategy: default_routing_strategy(),
        semantic_score: None,
    }
}

pub(super) fn with_edit_candidate(mut route: TaskRoute, candidate: EditCandidate) -> TaskRoute {
    route.edit_candidate = Some(candidate);
    route
}

pub(super) fn with_signals(mut route: TaskRoute, signals: &[&str]) -> TaskRoute {
    route.signals = signals.iter().map(|signal| (*signal).to_string()).collect();
    route
}

pub(super) fn with_strategy(
    mut route: TaskRoute,
    strategy: &str,
    semantic_score: Option<f32>,
) -> TaskRoute {
    route.routing_strategy = strategy.to_string();
    route.semantic_score = semantic_score;
    route
}

pub(super) fn default_routing_strategy() -> String {
    "keyword_fallback".to_string()
}
