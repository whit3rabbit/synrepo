//! Helpers for MCP responses that return sets of cards.

use serde_json::Value;

use crate::surface::card::accounting::{estimate_tokens_bytes, ContextAccounting};

use super::SynrepoState;

/// Apply an optional token cap to card JSON values.
pub fn apply_card_set_cap(
    cards: &mut Vec<Value>,
    budget_tokens: Option<usize>,
) -> (bool, Vec<ContextAccounting>) {
    let accountings = cards
        .iter()
        .map(|card| {
            let bytes = serde_json::to_vec(card).map(|v| v.len()).unwrap_or(0);
            ContextAccounting::new(
                crate::surface::card::Budget::Tiny,
                estimate_tokens_bytes(bytes),
                0,
                Vec::new(),
            )
        })
        .collect::<Vec<_>>();

    let Some(limit) = budget_tokens else {
        return (false, accountings);
    };

    let mut used = 0usize;
    let mut keep = cards.len();
    for (idx, accounting) in accountings.iter().enumerate() {
        let next = used.saturating_add(accounting.token_estimate);
        if next > limit && idx > 0 {
            keep = idx;
            break;
        }
        used = next;
    }
    let truncated = keep < cards.len();
    cards.truncate(keep);
    (truncated, accountings.into_iter().take(keep).collect())
}

/// Record metrics for a card-set response.
pub fn record_card_set_metrics(
    state: &SynrepoState,
    accountings: &[ContextAccounting],
    latency_ms: u64,
    stale: bool,
) {
    crate::pipeline::context_metrics::record_cards_best_effort(
        &crate::config::Config::synrepo_dir(&state.repo_root),
        accountings,
        latency_ms,
        stale,
    );
}
