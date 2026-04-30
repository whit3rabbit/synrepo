use std::collections::BTreeSet;
use std::time::Instant;

use serde_json::{json, Value};

use crate::surface::card::{Budget, ContextAccounting};

use super::SynrepoState;

pub(super) fn apply_pack_cap(
    artifacts: &mut Vec<Value>,
    omitted: &mut Vec<Value>,
    budget_tokens: Option<usize>,
) -> bool {
    let Some(cap) = budget_tokens else {
        return false;
    };
    let original = std::mem::take(artifacts);
    let mut kept = Vec::new();
    let mut total = 0usize;
    let mut truncated = false;
    for (idx, mut artifact) in original.into_iter().enumerate() {
        let tokens = artifact_tokens(&artifact);
        if total + tokens > cap && idx > 0 {
            omitted.push(json!({
                "target": artifact["target"].clone(),
                "artifact_type": artifact["artifact_type"].clone(),
                "reason": "budget_tokens_exceeded",
            }));
            truncated = true;
            continue;
        }
        if total + tokens > cap {
            mark_truncated(&mut artifact);
            truncated = true;
        }
        total += tokens;
        kept.push(artifact);
    }
    *artifacts = kept;
    truncated
}

pub(super) fn collect_artifact_accountings(artifacts: &[Value]) -> Vec<ContextAccounting> {
    artifacts
        .iter()
        .filter_map(|artifact| artifact.get("context_accounting"))
        .filter_map(|value| serde_json::from_value(value.clone()).ok())
        .collect()
}

pub(super) fn context_state(
    state: &SynrepoState,
    budget: Budget,
    accountings: &[ContextAccounting],
    truncation_applied: bool,
) -> Value {
    let mut source_hashes = BTreeSet::new();
    for accounting in accountings {
        for hash in &accounting.source_hashes {
            source_hashes.insert(hash.clone());
        }
    }
    json!({
        "graph_epoch": crate::structure::graph::snapshot::current(&state.repo_root)
            .map(|g| g.snapshot_epoch)
            .unwrap_or(0),
        "repo_root": state.repo_root,
        "source_hashes": source_hashes.into_iter().collect::<Vec<_>>(),
        "stale": accountings.iter().any(|a| a.stale),
        "budget_tier": budget,
        "token_estimate": accountings.iter().map(|a| a.token_estimate).sum::<usize>(),
        "raw_file_token_estimate": accountings
            .iter()
            .map(|a| a.raw_file_token_estimate)
            .sum::<usize>(),
        "truncation_applied": truncation_applied,
    })
}

pub(super) fn record_pack_metrics(
    state: &SynrepoState,
    accountings: &[ContextAccounting],
    start: Instant,
) {
    if accountings.is_empty() {
        return;
    }
    let synrepo_dir = crate::config::Config::synrepo_dir(&state.repo_root);
    let latency_ms = start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    crate::pipeline::context_metrics::record_cards_best_effort(
        &synrepo_dir,
        accountings,
        latency_ms,
        false,
    );
}

fn artifact_tokens(artifact: &Value) -> usize {
    artifact
        .get("context_accounting")
        .and_then(|v| v.get("token_estimate"))
        .and_then(|v| v.as_u64())
        .unwrap_or(1) as usize
}

fn mark_truncated(artifact: &mut Value) {
    if let Some(obj) = artifact
        .get_mut("context_accounting")
        .and_then(|v| v.as_object_mut())
    {
        obj.insert("truncation_applied".to_string(), Value::Bool(true));
    }
    if let Some(obj) = artifact
        .pointer_mut("/content/context_accounting")
        .and_then(|v| v.as_object_mut())
    {
        obj.insert("truncation_applied".to_string(), Value::Bool(true));
    }
}
