use serde_json::{json, Map, Value};

use super::{estimate_json_tokens, truncate_at_char_boundary, LARGE_ARRAY_PATHS};

const ROW_STRING_PREVIEW_CHARS: usize = 160;
const PRESERVED_ROW_FIELDS: &[&str] = &[
    "id",
    "node_id",
    "file_id",
    "symbol_id",
    "chunk_id",
    "path",
    "root_id",
    "is_primary_root",
    "root_kind",
    "root_label",
    "root_ref",
    "root_commit",
    "editable",
    "line",
    "kind",
    "edge_kind",
    "source",
    "fusion_score",
    "semantic_score",
    "target",
    "from",
    "to",
    "score",
    "severity",
    "status",
    "title",
];

pub(super) fn compact_over_budget_response(
    value: &Value,
    cap: usize,
    original_tokens: usize,
) -> Option<(Value, Vec<Value>)> {
    if let Some(search) = compact_search_response(value, cap, original_tokens) {
        return Some(search);
    }

    compact_large_arrays(value, original_tokens)
}

fn compact_search_response(
    value: &Value,
    cap: usize,
    original_tokens: usize,
) -> Option<(Value, Vec<Value>)> {
    if !is_search_shaped(value) {
        return None;
    }
    let compacted = super::super::compact::compact_search_response_forced(value, Some(cap));
    if estimate_json_tokens(&compacted) >= original_tokens {
        return None;
    }
    let omitted_count = compacted
        .pointer("/output_accounting/omitted_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    Some((
        compacted,
        vec![json!({
            "field": "results",
            "omitted_count": omitted_count,
            "reason": "response_token_cap",
            "strategy": "row_compaction",
        })],
    ))
}

fn is_search_shaped(value: &Value) -> bool {
    if value.get("query").is_none() {
        return false;
    }
    let Some(rows) = value.get("results").and_then(Value::as_array) else {
        return false;
    };
    !rows.is_empty()
        && rows.iter().all(|row| {
            row.as_object().is_some_and(|obj| {
                obj.get("path").and_then(Value::as_str).is_some()
                    && (obj.contains_key("content")
                        || obj.contains_key("line")
                        || obj.contains_key("file_id"))
            })
        })
}

fn compact_large_arrays(value: &Value, original_tokens: usize) -> Option<(Value, Vec<Value>)> {
    let mut compacted = value.clone();
    let mut omitted = Vec::new();
    for path in LARGE_ARRAY_PATHS {
        let Some(array) = compacted.pointer_mut(path).and_then(Value::as_array_mut) else {
            continue;
        };
        let compacted_count = compact_array_rows(array);
        if compacted_count == 0 {
            continue;
        }
        omitted.push(json!({
            "field": path.trim_start_matches('/'),
            "compacted_count": compacted_count,
            "reason": "response_token_cap",
            "strategy": "row_compaction",
        }));
    }
    if omitted.is_empty() || estimate_json_tokens(&compacted) >= original_tokens {
        return None;
    }
    Some((compacted, omitted))
}

fn compact_array_rows(array: &mut [Value]) -> usize {
    let mut compacted_count = 0;
    for row in array {
        let Value::Object(obj) = row else {
            continue;
        };
        let Some(compacted) = compact_row(obj) else {
            continue;
        };
        if Value::Object(compacted.clone()) == *row {
            continue;
        }
        *row = Value::Object(compacted);
        compacted_count += 1;
    }
    compacted_count
}

fn compact_row(row: &Map<String, Value>) -> Option<Map<String, Value>> {
    let mut compacted = Map::new();
    for field in PRESERVED_ROW_FIELDS {
        if let Some(value) = row.get(*field) {
            compacted.insert((*field).to_string(), compact_value(value));
        }
    }
    (!compacted.is_empty()).then_some(compacted)
}

fn compact_value(value: &Value) -> Value {
    match value {
        Value::String(text) if text.chars().count() > ROW_STRING_PREVIEW_CHARS => {
            let max_bytes = text
                .char_indices()
                .nth(ROW_STRING_PREVIEW_CHARS)
                .map(|(idx, _)| idx)
                .unwrap_or(text.len());
            Value::String(format!("{}...", truncate_at_char_boundary(text, max_bytes)))
        }
        _ => value.clone(),
    }
}

/// Keep a compact-search payload's parallel arrays index-aligned after generic
/// budget trimming. `compact_search_response_forced` emits `file_groups`,
/// `suggested_card_targets`, and `suggested_card_requests` as three equal-length
/// arrays mapped 1:1 by index, but `trim_known_large_fields` halves each path
/// independently, which would leave card targets/requests pointing at file
/// groups that were dropped. Array trimming keeps prefixes, so truncating every
/// present array to their common minimum length restores the mapping.
pub(super) fn realign_compact_search_arrays(value: &mut Value) {
    const PARALLEL: &[&str] = &[
        "file_groups",
        "suggested_card_targets",
        "suggested_card_requests",
    ];
    let Some(obj) = value.as_object() else {
        return;
    };
    // These parallel arrays only exist on a compact-search payload.
    if !obj.contains_key("file_groups") {
        return;
    }
    let Some(min_len) = PARALLEL
        .iter()
        .filter_map(|key| obj.get(*key).and_then(Value::as_array).map(Vec::len))
        .min()
    else {
        return;
    };
    let Some(obj) = value.as_object_mut() else {
        return;
    };
    for key in PARALLEL {
        if let Some(array) = obj.get_mut(*key).and_then(Value::as_array_mut) {
            array.truncate(min_len);
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::super::{clamp_json_response, clamp_response_string};

    fn has_row_compaction_marker(value: &Value) -> bool {
        value["response_omitted"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .any(|item| item["strategy"] == "row_compaction")
            })
            .unwrap_or(false)
    }

    #[test]
    fn response_budget_search_payload_uses_compact_shape() {
        let rows = (0..80)
            .map(|idx| {
                json!({
                    "path": format!("src/{idx}.rs"),
                    "line": idx,
                    "content": format!("TokenAlpha {}", "x".repeat(220)),
                    "root_id": "primary",
                    "editable": true,
                })
            })
            .collect::<Vec<_>>();
        let payload = json!({
            "query": "TokenAlpha",
            "engine": "syntext",
            "source_store": "substrate_index",
            "mode": "lexical",
            "semantic_available": false,
            "pattern_mode": "regex",
            "limit": 80,
            "filters": Value::Null,
            "result_count": 80,
            "results": rows,
        });

        let (value, report) = clamp_json_response(payload, Some(900));

        assert!(report.truncated);
        assert_eq!(value["output_mode"], "compact");
        assert!(value["file_groups"].as_array().is_some());
        assert!(value.get("results").is_none());
        assert!(has_row_compaction_marker(&value));
        assert_eq!(value["context_accounting"]["truncation_applied"], true);
    }

    #[test]
    fn response_budget_compacts_generic_rows_to_routing_fields() {
        let title = format!("{}{}", "important ".repeat(30), "tail");
        let edges = (0..20)
            .map(|idx| {
                json!({
                    "id": format!("edge_{idx}"),
                    "source": "sym_a",
                    "target": "sym_b",
                    "kind": "Calls",
                    "title": title,
                    "content": "x".repeat(600),
                    "metadata": { "nested": "y".repeat(600) },
                })
            })
            .collect::<Vec<_>>();

        let (value, report) = clamp_json_response(json!({ "edges": edges }), Some(1_500));
        let first = &value["edges"].as_array().unwrap()[0];

        assert!(report.truncated);
        assert_eq!(first["id"], "edge_0");
        assert_eq!(first["source"], "sym_a");
        assert_eq!(first["target"], "sym_b");
        assert_eq!(first["kind"], "Calls");
        assert!(first.get("content").is_none());
        assert!(first.get("metadata").is_none());
        assert!(first["title"].as_str().unwrap().ends_with("..."));
        assert!(has_row_compaction_marker(&value));
    }

    #[test]
    fn response_budget_preserves_semantic_only_search_rows() {
        let rows = vec![
            json!({
                "path": "src/lib.rs",
                "line": 12,
                "content": "x".repeat(5_000),
                "source": "lexical",
                "fusion_score": 0.1,
            }),
            json!({
                "path": Value::Null,
                "line": Value::Null,
                "content": Value::Null,
                "source": "semantic",
                "fusion_score": 0.2,
                "semantic_score": 0.8,
                "chunk_id": "chunk_1",
                "symbol_id": "sym_0000000000000001",
                "details": "y".repeat(5_000),
            }),
        ];
        let payload = json!({
            "query": "conceptual match",
            "engine": "syntext+vectors",
            "source_store": "substrate_index+vector_index",
            "mode": "auto",
            "semantic_available": true,
            "pattern_mode": "regex",
            "limit": 2,
            "filters": Value::Null,
            "result_count": 2,
            "results": rows,
        });

        let (value, report) = clamp_json_response(payload, Some(2_000));
        let results = value["results"].as_array().unwrap();
        let semantic = results
            .iter()
            .find(|row| row["source"] == "semantic")
            .expect("semantic-only row should remain routable");

        assert!(report.truncated);
        assert!(value.get("file_groups").is_none(), "{value}");
        assert_eq!(semantic["chunk_id"], "chunk_1");
        assert_eq!(semantic["symbol_id"], "sym_0000000000000001");
        assert_eq!(semantic["semantic_score"], 0.8);
        assert!(semantic.get("details").is_none());
        assert!(has_row_compaction_marker(&value));
    }

    #[test]
    fn response_budget_compaction_output_remains_valid_json() {
        let rows = (0..30)
            .map(|idx| {
                json!({
                    "node_id": format!("node_{idx}"),
                    "path": "src/lib.rs",
                    "status": "active",
                    "details": "z".repeat(500),
                })
            })
            .collect::<Vec<_>>();
        let clamped = clamp_response_string(&json!({ "nodes": rows }).to_string(), Some(800));

        serde_json::from_str::<Value>(&clamped.output).unwrap();
    }

    #[test]
    fn response_budget_keeps_compact_search_arrays_aligned() {
        let rows = (0..120)
            .map(|idx| {
                json!({
                    "path": format!("src/dir{}/file_{idx}.rs", idx % 9),
                    "line": idx,
                    "content": format!("Needle {}", "y".repeat(180)),
                    "root_id": "primary",
                    "editable": true,
                })
            })
            .collect::<Vec<_>>();
        let payload = json!({
            "query": "Needle",
            "engine": "syntext",
            "source_store": "substrate_index",
            "mode": "lexical",
            "result_count": 120,
            "results": rows,
        });

        let (value, report) = clamp_json_response(payload, Some(700));

        assert!(report.truncated);
        let groups = value["file_groups"].as_array().expect("file_groups").len();
        let targets = value["suggested_card_targets"]
            .as_array()
            .expect("suggested_card_targets")
            .len();
        let requests = value["suggested_card_requests"]
            .as_array()
            .expect("suggested_card_requests")
            .len();
        assert!(groups < 120, "file_groups should have been trimmed");
        assert_eq!(groups, targets, "card targets must match file groups");
        assert_eq!(groups, requests, "card requests must match file groups");
    }

    #[test]
    fn response_budget_inflation_guard_keeps_trim_fallback() {
        let rows = (0..300)
            .map(|idx| json!({ "id": format!("n{idx}") }))
            .collect::<Vec<_>>();
        let (value, report) = clamp_json_response(json!({ "nodes": rows }), Some(50));

        assert!(report.truncated);
        assert!(!has_row_compaction_marker(&value));
        assert!(value["nodes"].as_array().unwrap().len() < 300);
        assert_eq!(value["context_accounting"]["truncation_applied"], true);
        assert!(value["response_omitted"].as_array().unwrap().len() > 0);
    }
}
