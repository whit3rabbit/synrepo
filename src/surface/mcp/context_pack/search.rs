use serde_json::{json, Value};

use crate::surface::card::Budget;
use crate::surface::mcp::compact::{self, OutputMode};
use crate::surface::mcp::SynrepoState;

use super::{artifact, attach_estimated_accounting, ContextPackTarget};

pub(super) fn search_artifact(
    state: &SynrepoState,
    target: &ContextPackTarget,
    limit: usize,
    output_mode: OutputMode,
    budget_tokens: Option<usize>,
) -> crate::Result<Value> {
    let matches = crate::substrate::search(&state.config, &state.repo_root, &target.target)?;
    let results: Vec<Value> = matches
        .into_iter()
        .take(limit)
        .map(|m| {
            json!({
                "path": m.path.to_string_lossy(),
                "line": m.line_number,
                "content": String::from_utf8_lossy(&m.line_content).trim_end().to_string(),
            })
        })
        .collect();
    let result_count = results.len();
    let mut content = json!({ "query": target.target, "results": results });
    if output_mode == OutputMode::Compact {
        let compact_source = json!({
            "query": content["query"].clone(),
            "results": content["results"].clone(),
            "engine": "syntext",
            "source_store": "substrate_index",
            "limit": limit,
            "filters": Value::Null,
            "result_count": result_count,
        });
        content = compact::compact_search_response(&compact_source, budget_tokens);
        compact::record_output_accounting(state, &content);
    }
    attach_estimated_accounting(&mut content, Budget::Tiny);
    Ok(artifact("search", target, content))
}
