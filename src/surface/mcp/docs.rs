use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use crate::{config::Config, pipeline::explain::docs::search_commentary_docs};

use super::{
    helpers::render_result,
    limits::{bounded_limit_value, DEFAULT_SEARCH_LIMIT, MAX_SEARCH_LIMIT},
    SynrepoState,
};

/// Parameters for the `synrepo_docs_search` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct DocsSearchParams {
    pub repo_root: Option<std::path::PathBuf>,
    /// Lexical query string against explaind commentary docs.
    pub query: String,
    /// Maximum number of results to return. Defaults to 10, capped at 50.
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_limit() -> u32 {
    DEFAULT_SEARCH_LIMIT as u32
}

pub fn handle_docs_search(state: &SynrepoState, query: String, limit: u32) -> String {
    let result: anyhow::Result<serde_json::Value> = (|| {
        let compiler = state
            .create_read_compiler()
            .map_err(|error| anyhow::anyhow!(error))?;
        let synrepo_dir = Config::synrepo_dir(&state.repo_root);
        let overlay_dir = synrepo_dir.join("overlay");
        state.require_overlay_materialized()?;
        let overlay = crate::store::overlay::SqliteOverlayStore::open_existing(&overlay_dir)?;
        let limit = bounded_limit_value(limit as usize, DEFAULT_SEARCH_LIMIT, MAX_SEARCH_LIMIT);
        let results = compiler.with_reader(|graph| {
            search_commentary_docs(&synrepo_dir, graph, Some(&overlay), &query, limit)
        })?;

        Ok(json!({
            "query": query,
            "limit": limit,
            "results": results,
        }))
    })();
    render_result(result)
}
