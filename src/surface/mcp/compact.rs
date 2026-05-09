use std::collections::HashMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::surface::card::accounting::estimate_tokens_bytes;

use super::SynrepoState;

const MAX_LINES_PER_FILE: usize = 5;
const PREVIEW_CHARS: usize = 160;
const ARRAY_ITEM_OVERHEAD_TOKENS: usize = 1;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, JsonSchema, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum OutputMode {
    Default,
    #[default]
    Compact,
    Cards,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct OutputAccounting {
    pub returned_token_estimate: usize,
    pub original_token_estimate: usize,
    pub estimated_tokens_saved: usize,
    pub estimated_savings_ratio: f64,
    pub omitted_count: usize,
    pub truncation_applied: bool,
}

impl OutputAccounting {
    fn new(
        returned_token_estimate: usize,
        original_token_estimate: usize,
        omitted_count: usize,
        truncation_applied: bool,
    ) -> Self {
        let estimated_tokens_saved =
            original_token_estimate.saturating_sub(returned_token_estimate);
        let estimated_savings_ratio = if original_token_estimate > 0 {
            estimated_tokens_saved as f64 / original_token_estimate as f64
        } else {
            0.0
        };
        Self {
            returned_token_estimate,
            original_token_estimate,
            estimated_tokens_saved,
            estimated_savings_ratio,
            omitted_count,
            truncation_applied,
        }
    }
}

#[derive(Clone, Debug)]
struct SearchGroup {
    path: String,
    root_id: Option<String>,
    is_primary_root: Option<bool>,
    file_id: Option<String>,
    match_count: usize,
    lines: Vec<Value>,
}

pub fn compact_search_response(default_response: &Value, budget_tokens: Option<usize>) -> Value {
    let original_token_estimate = estimate_json_tokens(default_response);
    let groups = grouped_search_rows(default_response);
    let total_matches = default_response
        .get("result_count")
        .and_then(Value::as_u64)
        .unwrap_or(groups.iter().map(|g| g.match_count).sum::<usize>() as u64)
        as usize;
    if total_matches == 0 {
        return attach_accounting(
            minimal_miss_payload(default_response),
            original_token_estimate,
            0,
            false,
        );
    }

    let (kept, selection_truncated) = if let Some(cap) = budget_tokens {
        select_groups_for_budget(default_response, &groups, total_matches, cap)
    } else {
        (groups.clone(), false)
    };

    let omitted_count = omitted_count(total_matches, &kept);
    let payload = compact_payload(default_response, &groups, &kept, omitted_count);
    let returned_token_estimate = estimate_json_tokens(&payload);
    let budget_truncated =
        selection_truncated || budget_tokens.is_some_and(|cap| returned_token_estimate > cap);
    let compact = attach_accounting(
        payload,
        original_token_estimate,
        omitted_count,
        budget_truncated || omitted_count > 0,
    );
    if total_matches <= 3 {
        let fallback = attach_accounting(
            adaptive_raw_payload(default_response),
            original_token_estimate,
            0,
            false,
        );
        if estimate_json_tokens(&fallback) < estimate_json_tokens(&compact) {
            return fallback;
        }
    }
    compact
}

fn select_groups_for_budget(
    original: &Value,
    groups: &[SearchGroup],
    total_matches: usize,
    cap: usize,
) -> (Vec<SearchGroup>, bool) {
    let base_payload = compact_payload(original, groups, &[], total_matches);
    let mut estimated_tokens = estimate_json_tokens(&base_payload);
    let mut kept = Vec::new();

    for group in groups {
        let group_tokens = estimate_json_tokens(&group_json(group));
        let target_tokens = estimate_json_tokens(&Value::String(group.path.clone()));
        let next_tokens = estimated_tokens
            .saturating_add(group_tokens)
            .saturating_add(target_tokens)
            .saturating_add(ARRAY_ITEM_OVERHEAD_TOKENS);
        if next_tokens > cap && !kept.is_empty() {
            break;
        }
        kept.push(group.clone());
        estimated_tokens = next_tokens;
    }

    let truncated = kept.len() < groups.len() || estimated_tokens > cap;
    (kept, truncated)
}

pub fn output_accounting(value: &Value) -> Option<OutputAccounting> {
    value
        .get("output_accounting")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
}

pub fn record_output_accounting(state: &SynrepoState, value: &Value) {
    let Some(accounting) = output_accounting(value) else {
        return;
    };
    let synrepo_dir = crate::config::Config::synrepo_dir(&state.repo_root);
    crate::pipeline::context_metrics::record_compact_output_best_effort(
        &synrepo_dir,
        accounting.returned_token_estimate,
        accounting.original_token_estimate,
        accounting.estimated_tokens_saved,
        accounting.omitted_count,
        accounting.truncation_applied,
    );
}

fn estimate_json_tokens(value: &Value) -> usize {
    let bytes = serde_json::to_vec(value).map(|v| v.len()).unwrap_or(4);
    estimate_tokens_bytes(bytes)
}

fn grouped_search_rows(response: &Value) -> Vec<SearchGroup> {
    let mut groups = Vec::<SearchGroup>::new();
    let mut indexes = HashMap::<String, usize>::new();
    let Some(rows) = response.get("results").and_then(Value::as_array) else {
        return groups;
    };
    for row in rows {
        let Some(path) = row.get("path").and_then(Value::as_str) else {
            continue;
        };
        let root_id = row
            .get("root_id")
            .and_then(Value::as_str)
            .map(str::to_string);
        let key = format!("{}\0{path}", root_id.as_deref().unwrap_or("primary"));
        let idx = if let Some(idx) = indexes.get(&key) {
            *idx
        } else {
            let idx = groups.len();
            indexes.insert(key, idx);
            groups.push(SearchGroup {
                path: path.to_string(),
                root_id,
                is_primary_root: row.get("is_primary_root").and_then(Value::as_bool),
                file_id: row
                    .get("file_id")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                match_count: 0,
                lines: Vec::new(),
            });
            idx
        };
        let group = &mut groups[idx];
        group.match_count += 1;
        if group.lines.len() >= MAX_LINES_PER_FILE {
            continue;
        }
        group.lines.push(json!({
            "line": row.get("line").cloned().unwrap_or(Value::Null),
            "preview": row
                .get("content")
                .and_then(Value::as_str)
                .map(truncate_preview)
                .unwrap_or_default(),
        }));
    }
    groups
}

fn compact_payload(
    original: &Value,
    all_groups: &[SearchGroup],
    kept_groups: &[SearchGroup],
    omitted_count: usize,
) -> Value {
    let omitted_file_count = all_groups.len().saturating_sub(kept_groups.len());
    json!({
        "query": original.get("query").cloned().unwrap_or(Value::Null),
        "engine": original.get("engine").cloned().unwrap_or(Value::Null),
        "source_store": original.get("source_store").cloned().unwrap_or(Value::Null),
        "mode": original.get("mode").cloned().unwrap_or(Value::Null),
        "semantic_available": original.get("semantic_available").cloned().unwrap_or(Value::Null),
        "pattern_mode": original.get("pattern_mode").cloned().unwrap_or(Value::Null),
        "warnings": original.get("warnings").cloned().unwrap_or(Value::Null),
        "limit": original.get("limit").cloned().unwrap_or(Value::Null),
        "filters": original.get("filters").cloned().unwrap_or(Value::Null),
        "result_count": original.get("result_count").cloned().unwrap_or(Value::Null),
        "output_mode": "compact",
        "file_groups": kept_groups.iter().map(group_json).collect::<Vec<_>>(),
        "suggested_card_targets": kept_groups
            .iter()
            .map(|g| g.path.clone())
            .collect::<Vec<_>>(),
        "suggested_card_requests": kept_groups
            .iter()
            .map(card_request_json)
            .collect::<Vec<_>>(),
        "omitted": {
            "match_count": omitted_count,
            "file_count": omitted_file_count,
        },
    })
}

fn minimal_miss_payload(original: &Value) -> Value {
    json!({
        "query": original.get("query").cloned().unwrap_or(Value::Null),
        "engine": original.get("engine").cloned().unwrap_or(Value::Null),
        "source_store": original.get("source_store").cloned().unwrap_or(Value::Null),
        "mode": original.get("mode").cloned().unwrap_or(Value::Null),
        "semantic_available": original.get("semantic_available").cloned().unwrap_or(Value::Null),
        "pattern_mode": original.get("pattern_mode").cloned().unwrap_or(Value::Null),
        "warnings": original.get("warnings").cloned().unwrap_or(Value::Null),
        "result_count": 0,
        "output_mode": "compact",
        "suggested_card_targets": [],
        "miss_reason": "no_matches",
    })
}

fn adaptive_raw_payload(original: &Value) -> Value {
    let mut value = original.clone();
    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "output_mode".to_string(),
            Value::String("default".to_string()),
        );
    }
    value
}

fn attach_accounting(
    mut payload: Value,
    original_token_estimate: usize,
    omitted_count: usize,
    truncation_applied: bool,
) -> Value {
    let returned_token_estimate = estimate_json_tokens(&payload);
    let accounting = OutputAccounting::new(
        returned_token_estimate,
        original_token_estimate,
        omitted_count,
        truncation_applied,
    );
    if let Some(obj) = payload.as_object_mut() {
        obj.insert(
            "output_accounting".to_string(),
            serde_json::to_value(accounting).unwrap_or(Value::Null),
        );
    }
    payload
}

fn group_json(group: &SearchGroup) -> Value {
    json!({
        "path": group.path,
        "root_id": group.root_id,
        "is_primary_root": group.is_primary_root,
        "file_id": group.file_id,
        "match_count": group.match_count,
        "returned_line_count": group.lines.len(),
        "lines": group.lines,
        "card_target": group.path,
    })
}

fn card_request_json(group: &SearchGroup) -> Value {
    json!({
        "target": group.file_id.as_deref().unwrap_or(&group.path),
        "path": group.path,
        "root_id": group.root_id,
        "file_id": group.file_id,
    })
}

fn omitted_count(total_matches: usize, kept: &[SearchGroup]) -> usize {
    let returned = kept.iter().map(|g| g.lines.len()).sum::<usize>();
    total_matches.saturating_sub(returned)
}

fn truncate_preview(content: &str) -> String {
    let trimmed = content.trim();
    match trimmed.char_indices().nth(PREVIEW_CHARS) {
        None => trimmed.to_string(),
        Some((end, _)) => format!("{}...", &trimmed[..end]),
    }
}
