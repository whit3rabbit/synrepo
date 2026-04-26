use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use crate::{config::Config, pipeline::explain::docs::search_commentary_docs};

use super::{helpers::render_result, SynrepoState};

/// Parameters for the `synrepo_docs_search` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct DocsSearchParams {
        pub repo_root: Option<std::path::PathBuf>,
/// Lexical query string against explaind commentary docs.
    pub query: String,
    /// Maximum number of results to return. Defaults to 20.
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_limit() -> u32 {
    20
}

pub fn handle_docs_search(state: &SynrepoState, query: String, limit: u32) -> String {
    let result: anyhow::Result<serde_json::Value> = (|| {
        let compiler = state
            .create_read_compiler()
            .map_err(|error| anyhow::anyhow!(error))?;
        let synrepo_dir = Config::synrepo_dir(&state.repo_root);
        let overlay_dir = synrepo_dir.join("overlay");
        let overlay = crate::store::overlay::SqliteOverlayStore::open_existing(&overlay_dir).ok();
        let results = compiler.with_reader(|graph| {
            search_commentary_docs(
                &synrepo_dir,
                graph,
                overlay.as_ref(),
                &query,
                limit as usize,
            )
        })?;

        Ok(json!({
            "query": query,
            "results": results,
        }))
    })();
    render_result(result)
}
