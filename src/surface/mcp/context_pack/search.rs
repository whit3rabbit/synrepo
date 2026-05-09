use serde_json::{json, Value};
use syntext::SearchOptions;

use crate::surface::card::Budget;
use crate::surface::mcp::compact::{self, OutputMode};
use crate::surface::mcp::SynrepoState;

use super::artifacts::{artifact, attach_estimated_accounting};
use super::ContextPackTarget;

pub(super) fn search_artifact(
    state: &SynrepoState,
    target: &ContextPackTarget,
    limit: usize,
    output_mode: OutputMode,
    budget_tokens: Option<usize>,
) -> crate::Result<Value> {
    let report = crate::substrate::hybrid_search(
        &state.config,
        &state.repo_root,
        &target.target,
        &SearchOptions {
            max_results: Some(limit),
            ..SearchOptions::default()
        },
    )?;
    let results: Vec<Value> = report
        .rows
        .into_iter()
        .map(|row| {
            json!({
                "path": row.path,
                "line": row.line,
                "content": row.content,
                "source": row.source.as_str(),
                "fusion_score": row.fusion_score,
                "semantic_score": row.semantic_score,
                "chunk_id": row.chunk_id,
                "symbol_id": row.symbol_id,
            })
        })
        .collect();
    let result_count = results.len();
    let source_store = if report.semantic_available {
        "substrate_index+vector_index"
    } else {
        "substrate_index"
    };
    let mut content = json!({
        "query": target.target,
        "results": results,
        "engine": report.engine,
        "source_store": source_store,
        "mode": "auto",
        "semantic_available": report.semantic_available,
    });
    if output_mode == OutputMode::Compact {
        let compact_source = json!({
            "query": content["query"].clone(),
            "results": content["results"].clone(),
            "engine": content["engine"].clone(),
            "source_store": content["source_store"].clone(),
            "mode": content["mode"].clone(),
            "semantic_available": content["semantic_available"].clone(),
            "pattern_mode": "regex",
            "limit": limit,
            "filters": Value::Null,
            "result_count": result_count,
        });
        content = compact::compact_search_response_forced(&compact_source, budget_tokens);
        compact::record_output_accounting(state, &content);
    }
    attach_estimated_accounting(&mut content, Budget::Tiny);
    Ok(artifact("search", target, content))
}
