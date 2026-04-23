use std::time::Instant;

use super::SynrepoState;

pub fn finalize_card_json(
    state: &SynrepoState,
    mut json: serde_json::Value,
    budget_tokens: Option<usize>,
    start: Instant,
    test_surface_hit: bool,
) -> serde_json::Value {
    apply_numeric_cap_marker(&mut json, budget_tokens);
    record_embedded_card_metrics(state, &json, start, test_surface_hit);
    json
}

pub fn record_embedded_card_metrics(
    state: &SynrepoState,
    json: &serde_json::Value,
    start: Instant,
    test_surface_hit: bool,
) {
    let Some(accounting_value) = json
        .pointer("/context_accounting")
        .or_else(|| json.pointer("/focal_card/context_accounting"))
    else {
        return;
    };
    let Ok(accounting) =
        serde_json::from_value::<crate::surface::card::ContextAccounting>(accounting_value.clone())
    else {
        return;
    };
    let synrepo_dir = crate::config::Config::synrepo_dir(&state.repo_root);
    let latency_ms = start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    crate::pipeline::context_metrics::record_card_best_effort(
        &synrepo_dir,
        &accounting,
        latency_ms,
        test_surface_hit,
    );
}

fn apply_numeric_cap_marker(json: &mut serde_json::Value, budget_tokens: Option<usize>) {
    let Some(cap) = budget_tokens else {
        return;
    };
    let Some(accounting) = json
        .as_object_mut()
        .and_then(|obj| obj.get_mut("context_accounting"))
        .and_then(|v| v.as_object_mut())
    else {
        return;
    };
    let token_estimate = accounting
        .get("token_estimate")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    if token_estimate > cap {
        accounting.insert(
            "truncation_applied".to_string(),
            serde_json::Value::Bool(true),
        );
    }
}
