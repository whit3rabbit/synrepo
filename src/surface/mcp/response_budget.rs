//! Final response budgeting for MCP tool output.

use std::path::Path;

use serde_json::{json, Value};

use super::limits::{BYTES_PER_TOKEN_ESTIMATE, DEFAULT_RESPONSE_TOKEN_CAP, MAX_RESPONSE_TOKEN_CAP};

const LARGE_ARRAY_PATHS: &[&str] = &[
    "/results",
    "/cards",
    "/artifacts",
    "/edges",
    "/nodes",
    "/suggestions",
    "/activities",
    "/overlay/links",
    "/node/path_history",
];

#[derive(Clone, Debug, Default)]
pub struct ResponseBudgetReport {
    pub token_estimate: usize,
    pub returned_token_estimate: usize,
    pub token_cap: usize,
    pub over_soft_cap: bool,
    pub truncated: bool,
}

impl ResponseBudgetReport {
    pub fn should_record(&self) -> bool {
        self.token_estimate > 0
    }
}

#[derive(Clone, Debug)]
pub struct ClampedResponse {
    pub output: String,
    pub report: ResponseBudgetReport,
}

pub fn estimate_json_tokens(value: &Value) -> usize {
    serde_json::to_vec(value)
        .map(|bytes| estimate_tokens_bytes(bytes.len()))
        .unwrap_or(1)
}

pub fn estimate_tokens_bytes(byte_len: usize) -> usize {
    (byte_len / BYTES_PER_TOKEN_ESTIMATE).max(1)
}

pub fn clamp_response_string(output: &str, requested_budget: Option<usize>) -> ClampedResponse {
    let cap = effective_cap(requested_budget);
    let Ok(value) = serde_json::from_str::<Value>(output) else {
        let estimate = estimate_tokens_bytes(output.len());
        return ClampedResponse {
            output: output.to_string(),
            report: ResponseBudgetReport {
                token_estimate: estimate,
                returned_token_estimate: estimate,
                token_cap: cap,
                over_soft_cap: estimate > DEFAULT_RESPONSE_TOKEN_CAP,
                truncated: false,
            },
        };
    };
    let (value, report) = clamp_json_response(value, requested_budget);
    let output = serde_json::to_string_pretty(&value).unwrap_or_else(|_| output.to_string());
    ClampedResponse { output, report }
}

pub fn clamp_and_record_response(synrepo_dir: &Path, tool: &str, output: String) -> String {
    let clamped = clamp_response_string(&output, None);
    if clamped.report.should_record() {
        crate::pipeline::context_metrics::record_mcp_response_budget_best_effort(
            synrepo_dir,
            tool,
            clamped.report.token_estimate,
            clamped.report.over_soft_cap,
            clamped.report.truncated,
        );
    }
    clamped.output
}

pub fn clamp_json_response(
    mut value: Value,
    requested_budget: Option<usize>,
) -> (Value, ResponseBudgetReport) {
    let cap = effective_cap(requested_budget);
    let original_tokens = estimate_json_tokens(&value);
    if original_tokens <= cap {
        return (
            value,
            ResponseBudgetReport {
                token_estimate: original_tokens,
                returned_token_estimate: original_tokens,
                token_cap: cap,
                over_soft_cap: original_tokens > DEFAULT_RESPONSE_TOKEN_CAP,
                truncated: false,
            },
        );
    }

    let mut omitted = Vec::new();
    trim_known_large_fields(&mut value, cap, &mut omitted);
    attach_context_accounting(&mut value, original_tokens, cap, true);
    attach_omitted(&mut value, omitted);
    let returned_tokens = estimate_json_tokens(&value);
    (
        value,
        ResponseBudgetReport {
            token_estimate: original_tokens,
            returned_token_estimate: returned_tokens,
            token_cap: cap,
            over_soft_cap: original_tokens > DEFAULT_RESPONSE_TOKEN_CAP,
            truncated: true,
        },
    )
}

fn effective_cap(requested_budget: Option<usize>) -> usize {
    requested_budget
        .unwrap_or(DEFAULT_RESPONSE_TOKEN_CAP)
        .clamp(1, MAX_RESPONSE_TOKEN_CAP)
}

fn trim_known_large_fields(value: &mut Value, cap: usize, omitted: &mut Vec<Value>) {
    for path in LARGE_ARRAY_PATHS {
        while estimate_json_tokens(value) > cap && trim_array_at_path(value, path, omitted) {}
    }
    if estimate_json_tokens(value) <= cap {
        return;
    }
    truncate_large_strings(value, omitted);
}

fn trim_array_at_path(value: &mut Value, path: &str, omitted: &mut Vec<Value>) -> bool {
    let Some(array) = value.pointer_mut(path).and_then(Value::as_array_mut) else {
        return false;
    };
    if array.len() <= 1 {
        return false;
    }
    let original = array.len();
    let keep = (original / 2).max(1);
    array.truncate(keep);
    omitted.push(json!({
        "field": path.trim_start_matches('/'),
        "omitted_count": original.saturating_sub(keep),
        "reason": "response_token_cap",
    }));
    true
}

fn truncate_large_strings(value: &mut Value, omitted: &mut Vec<Value>) {
    match value {
        Value::String(text) if text.len() > 1_000 => {
            text.truncate(1_000);
            text.push_str("...");
            omitted.push(json!({
                "field": "large_string",
                "reason": "response_token_cap",
            }));
        }
        Value::Array(items) => {
            for item in items {
                truncate_large_strings(item, omitted);
            }
        }
        Value::Object(map) => {
            for item in map.values_mut() {
                truncate_large_strings(item, omitted);
            }
        }
        _ => {}
    }
}

fn attach_context_accounting(
    value: &mut Value,
    original_tokens: usize,
    cap: usize,
    truncated: bool,
) {
    let Some(obj) = value.as_object_mut() else {
        return;
    };
    let mut accounting = obj
        .remove("context_accounting")
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}));
    if let Some(map) = accounting.as_object_mut() {
        map.insert("token_estimate".to_string(), json!(original_tokens));
        map.insert("token_cap".to_string(), json!(cap));
        map.insert("truncation_applied".to_string(), json!(truncated));
        map.insert(
            "truncation_reason".to_string(),
            json!("response exceeded MCP response token cap"),
        );
    }
    obj.insert("context_accounting".to_string(), accounting);
}

fn attach_omitted(value: &mut Value, omitted: Vec<Value>) {
    if omitted.is_empty() {
        return;
    }
    let Some(obj) = value.as_object_mut() else {
        return;
    };
    obj.insert("response_omitted".to_string(), Value::Array(omitted));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oversized_response_trims_known_arrays() {
        let rows = (0..100)
            .map(|idx| json!({ "path": "src/lib.rs", "content": "x".repeat(200), "idx": idx }))
            .collect::<Vec<_>>();
        let (value, report) = clamp_json_response(json!({ "results": rows }), Some(200));

        assert!(report.truncated);
        assert!(value["results"].as_array().unwrap().len() < 100);
        assert_eq!(value["context_accounting"]["truncation_applied"], true);
        assert!(value["response_omitted"].as_array().unwrap().len() > 0);
    }
}
