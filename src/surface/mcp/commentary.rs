use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use crate::{
    core::ids::NodeId,
    pipeline::{
        explain::{build_commentary_generator, telemetry},
        writer::{acquire_write_admission, map_lock_error},
    },
    surface::commentary_scope::{self, CommentaryRefreshScope},
};

use super::{helpers::render_result, SynrepoState};

/// Parameters for the `synrepo_refresh_commentary` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct RefreshCommentaryParams {
    pub repo_root: Option<std::path::PathBuf>,
    /// Target: qualified symbol name, path, or node ID.
    #[serde(default)]
    pub target: Option<String>,
    /// Scope to refresh: target (default), file, directory, or stale.
    #[serde(default)]
    pub scope: RefreshScope,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, JsonSchema, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RefreshScope {
    #[default]
    Target,
    File,
    Directory,
    Stale,
}

pub fn handle_refresh_commentary(state: &SynrepoState, target: String) -> String {
    handle_refresh_commentary_params(
        state,
        RefreshCommentaryParams {
            repo_root: None,
            target: Some(target),
            scope: RefreshScope::Target,
        },
        None,
    )
}

pub fn handle_refresh_commentary_params(
    state: &SynrepoState,
    params: RefreshCommentaryParams,
    mut progress: Option<&mut dyn FnMut(serde_json::Value)>,
) -> String {
    let synrepo_dir = crate::config::Config::synrepo_dir(&state.repo_root);
    let result = telemetry::with_synrepo_dir(&synrepo_dir, || {
        state.require_overlay_available()?;
        let _writer_lock = acquire_write_admission(&synrepo_dir, "refresh commentary")
            .map_err(|err| map_lock_error("refresh commentary", err))?;
        let compiler = state
            .create_sqlite_compiler()
            .map_err(|e| anyhow::anyhow!(e))?;
        let max_tokens = state.config.commentary_cost_limit;
        let generator = build_commentary_generator(&state.config, max_tokens);
        let node_ids = resolve_refresh_nodes(state, &compiler, &params)?;
        let total = node_ids.len();
        let mut refreshed = 0usize;
        let mut skipped = 0usize;
        let mut results = Vec::new();

        for (idx, node_id) in node_ids.into_iter().enumerate() {
            emit_progress(&mut progress, idx + 1, total, node_id);
            match compiler.refresh_commentary(node_id, &*generator) {
                Ok(text) => {
                    if text.is_some() {
                        refreshed += 1;
                    } else {
                        skipped += 1;
                    }
                    results.push(json!({
                        "node_id": node_id.to_string(),
                        "status": if text.is_some() { "refreshed" } else { "skipped" },
                        "commentary": text,
                    }));
                }
                Err(error) => {
                    skipped += 1;
                    results.push(json!({
                        "node_id": node_id.to_string(),
                        "status": "error",
                        "error": error.to_string(),
                    }));
                }
            }
        }

        Ok(json!({
            "scope": params.scope.as_str(),
            "status": if refreshed > 0 { "refreshed" } else { "skipped" },
            "refreshed": refreshed,
            "skipped": skipped,
            "total": total,
            "results": results,
            "node_id": results.first().and_then(|value| value.get("node_id")).cloned(),
            "commentary": results.first().and_then(|value| value.get("commentary")).cloned(),
        }))
    });
    crate::pipeline::context_metrics::record_commentary_refresh_best_effort(
        &synrepo_dir,
        result.is_err(),
    );
    render_result(result)
}

fn resolve_refresh_nodes(
    state: &SynrepoState,
    compiler: &crate::surface::card::compiler::GraphCardCompiler,
    params: &RefreshCommentaryParams,
) -> anyhow::Result<Vec<NodeId>> {
    let synrepo_dir = crate::config::Config::synrepo_dir(&state.repo_root);
    commentary_scope::resolve_refresh_nodes(
        compiler,
        &synrepo_dir,
        params.scope.into(),
        params.target.as_deref(),
    )
}

fn emit_progress(
    progress: &mut Option<&mut dyn FnMut(serde_json::Value)>,
    current: usize,
    total: usize,
    node_id: NodeId,
) {
    if let Some(progress) = progress.as_mut() {
        progress(json!({
            "current": current,
            "total": total,
            "message": format!("refreshing commentary for {node_id}"),
        }));
    }
}

impl RefreshScope {
    fn as_str(self) -> &'static str {
        CommentaryRefreshScope::from(self).as_str()
    }
}

impl From<RefreshScope> for CommentaryRefreshScope {
    fn from(scope: RefreshScope) -> Self {
        match scope {
            RefreshScope::Target => Self::Target,
            RefreshScope::File => Self::File,
            RefreshScope::Directory => Self::Directory,
            RefreshScope::Stale => Self::Stale,
        }
    }
}
