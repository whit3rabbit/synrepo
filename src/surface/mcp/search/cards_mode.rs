use std::collections::HashSet;
use std::str::FromStr;

use serde_json::{json, Value};

use crate::core::ids::FileNodeId;
use crate::surface::card::{Budget, CardCompiler};

use crate::surface::mcp::{
    card_set::{apply_card_set_cap, record_card_set_metrics},
    SynrepoState,
};

pub(super) fn search_cards_response(
    state: &SynrepoState,
    response: &serde_json::Value,
    budget_tokens: Option<usize>,
) -> anyhow::Result<serde_json::Value> {
    let start = std::time::Instant::now();
    let rows = response
        .get("results")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    let mut seen = HashSet::new();
    let mut cards = Vec::new();
    let mut unresolved = Vec::new();

    state
        .with_read_compiler(|compiler| {
            for row in rows {
                let Some(path) = row.get("path").and_then(|value| value.as_str()) else {
                    unresolved.push(json!({
                        "reason": "missing_path",
                        "row": row,
                    }));
                    continue;
                };
                let root_id = row
                    .get("root_id")
                    .and_then(|value| value.as_str())
                    .unwrap_or("primary");
                let seen_key = format!("{root_id}\0{path}");
                if !seen.insert(seen_key) {
                    continue;
                }
                let file = row
                    .get("file_id")
                    .and_then(|value| value.as_str())
                    .and_then(|raw| FileNodeId::from_str(raw).ok())
                    .and_then(|id| compiler.reader().get_file(id).ok().flatten())
                    .or_else(|| {
                        compiler
                            .reader()
                            .file_by_root_path(root_id, path)
                            .ok()
                            .flatten()
                    });
                match file {
                    Some(file) => cards.push(
                        serde_json::to_value(compiler.file_card(file.id, Budget::Tiny)?)
                            .map_err(|err| crate::Error::Other(anyhow::anyhow!(err)))?,
                    ),
                    None => unresolved.push(json!({
                        "path": path,
                        "root_id": root_id,
                        "reason": "path_not_in_graph",
                    })),
                }
            }
            Ok(())
        })
        .map_err(|err| anyhow::anyhow!(err))?;

    let original_count = cards.len();
    let (truncation_applied, accountings) = apply_card_set_cap(&mut cards, budget_tokens);
    let latency_ms = start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    record_card_set_metrics(state, &accountings, latency_ms, false);

    let mut output = json!({
        "query": response.get("query").cloned().unwrap_or(Value::Null),
        "engine": response.get("engine").cloned().unwrap_or(Value::Null),
        "source_store": "graph",
        "search_source_store": response.get("source_store").cloned().unwrap_or(Value::Null),
        "mode": response.get("mode").cloned().unwrap_or(Value::Null),
        "semantic_available": response.get("semantic_available").cloned().unwrap_or(Value::Null),
        "pattern_mode": response.get("pattern_mode").cloned().unwrap_or(Value::Null),
        "output_mode": "cards",
        "cards": cards,
        "card_count": cards.len(),
        "truncation_applied": truncation_applied,
        "omitted": {
            "card_count": original_count.saturating_sub(cards.len()),
        },
        "unresolved": unresolved,
    });
    if let Some(warnings) = response.get("warnings") {
        if let Some(obj) = output.as_object_mut() {
            obj.insert("warnings".to_string(), warnings.clone());
        }
    }
    Ok(output)
}
