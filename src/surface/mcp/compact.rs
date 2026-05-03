use std::collections::HashMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::surface::card::accounting::estimate_tokens_bytes;

use super::SynrepoState;

const MAX_LINES_PER_FILE: usize = 5;
const PREVIEW_CHARS: usize = 160;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, JsonSchema, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum OutputMode {
    #[default]
    Default,
    Compact,
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

    let mut kept = groups.clone();
    let budget_truncated = if let Some(cap) = budget_tokens {
        loop {
            let omitted_count = omitted_count(total_matches, &kept);
            let candidate = compact_payload(default_response, &groups, &kept, omitted_count);
            if estimate_json_tokens(&candidate) <= cap || kept.len() <= 1 {
                break estimate_json_tokens(&candidate) > cap;
            }
            kept.pop();
        }
    } else {
        false
    };

    let omitted_count = omitted_count(total_matches, &kept);
    let mut payload = compact_payload(default_response, &groups, &kept, omitted_count);
    let returned_token_estimate = estimate_json_tokens(&payload);
    let accounting = OutputAccounting::new(
        returned_token_estimate,
        original_token_estimate,
        omitted_count,
        budget_truncated || omitted_count > 0,
    );
    if let Some(obj) = payload.as_object_mut() {
        obj.insert(
            "output_accounting".to_string(),
            serde_json::to_value(accounting).unwrap_or(Value::Null),
        );
    }
    payload
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
        let idx = if let Some(idx) = indexes.get(path) {
            *idx
        } else {
            let idx = groups.len();
            indexes.insert(path.to_string(), idx);
            groups.push(SearchGroup {
                path: path.to_string(),
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
        "limit": original.get("limit").cloned().unwrap_or(Value::Null),
        "filters": original.get("filters").cloned().unwrap_or(Value::Null),
        "result_count": original.get("result_count").cloned().unwrap_or(Value::Null),
        "output_mode": "compact",
        "file_groups": kept_groups.iter().map(group_json).collect::<Vec<_>>(),
        "suggested_card_targets": kept_groups
            .iter()
            .map(|g| g.path.clone())
            .collect::<Vec<_>>(),
        "omitted": {
            "match_count": omitted_count,
            "file_count": omitted_file_count,
        },
    })
}

fn group_json(group: &SearchGroup) -> Value {
    json!({
        "path": group.path,
        "match_count": group.match_count,
        "returned_line_count": group.lines.len(),
        "lines": group.lines,
        "card_target": group.path,
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
