use std::path::PathBuf;

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use crate::{
    core::ids::{FileNodeId, NodeId},
    pipeline::{
        explain::{build_commentary_generator, telemetry},
        repair::load_commentary_work_plan,
        writer::{acquire_write_admission, map_lock_error},
    },
    surface::card::CardCompiler,
};

use super::{error::McpError, helpers::render_result, SynrepoState};

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
    match params.scope {
        RefreshScope::Target => {
            let target = required_target(params)?;
            let node_id = compiler
                .resolve_target(target)?
                .ok_or_else(|| McpError::not_found(format!("target not found: {target}")))?;
            Ok(vec![node_id])
        }
        RefreshScope::File => file_scope_nodes(compiler, required_target(params)?),
        RefreshScope::Directory => directory_scope_nodes(state, required_target(params)?),
        RefreshScope::Stale => stale_scope_nodes(state),
    }
}

fn required_target(params: &RefreshCommentaryParams) -> anyhow::Result<&str> {
    params
        .target
        .as_deref()
        .filter(|target| !target.trim().is_empty())
        .ok_or_else(|| McpError::invalid_parameter("target is required for this scope").into())
}

fn file_scope_nodes(
    compiler: &crate::surface::card::compiler::GraphCardCompiler,
    target: &str,
) -> anyhow::Result<Vec<NodeId>> {
    let node_id = compiler
        .resolve_target(target)?
        .ok_or_else(|| McpError::not_found(format!("target not found: {target}")))?;
    let file_id = match node_id {
        NodeId::File(file_id) => file_id,
        NodeId::Symbol(sym_id) => compiler
            .reader()
            .get_symbol(sym_id)?
            .map(|symbol| symbol.file_id)
            .ok_or_else(|| McpError::not_found(format!("symbol not found: {target}")))?,
        NodeId::Concept(_) => {
            return Err(
                McpError::invalid_parameter("file scope requires a file or symbol target").into(),
            )
        }
    };
    file_and_symbol_nodes(compiler, file_id)
}

fn file_and_symbol_nodes(
    compiler: &crate::surface::card::compiler::GraphCardCompiler,
    file_id: FileNodeId,
) -> anyhow::Result<Vec<NodeId>> {
    let mut nodes = vec![NodeId::File(file_id)];
    for symbol in compiler.reader().symbols_for_file(file_id)? {
        nodes.push(NodeId::Symbol(symbol.id));
    }
    Ok(nodes)
}

fn directory_scope_nodes(state: &SynrepoState, target: &str) -> anyhow::Result<Vec<NodeId>> {
    let synrepo_dir = crate::config::Config::synrepo_dir(&state.repo_root);
    let scope = [PathBuf::from(target)];
    let plan = load_commentary_work_plan(&synrepo_dir, Some(&scope))?;
    Ok(plan
        .refresh
        .into_iter()
        .chain(plan.file_seeds)
        .chain(plan.symbol_seed_candidates)
        .map(|item| item.node_id)
        .collect())
}

fn stale_scope_nodes(state: &SynrepoState) -> anyhow::Result<Vec<NodeId>> {
    let synrepo_dir = crate::config::Config::synrepo_dir(&state.repo_root);
    let plan = load_commentary_work_plan(&synrepo_dir, None)?;
    Ok(plan.refresh.into_iter().map(|item| item.node_id).collect())
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
        match self {
            Self::Target => "target",
            Self::File => "file",
            Self::Directory => "directory",
            Self::Stale => "stale",
        }
    }
}
