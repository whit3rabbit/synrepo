use std::collections::HashSet;

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use crate::{
    core::ids::NodeId,
    structure::graph::EdgeKind,
    surface::card::{Budget, CardCompiler},
};

use super::helpers::{render_result, with_graph_snapshot};
use super::SynrepoState;

/// Parameters for the `synrepo_search` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchParams {
    /// Lexical query string.
    pub query: String,
    /// Maximum number of results to return. Defaults to 20.
    #[serde(default = "default_limit")]
    pub limit: u32,
}

pub fn default_limit() -> u32 {
    20
}

/// Parameters for the `synrepo_where_to_edit` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct WhereToEditParams {
    /// Plain-language description of the task (e.g. "add retry logic to HTTP client").
    pub task: String,
    /// Maximum number of file suggestions to return. Defaults to 5.
    #[serde(default = "default_edit_limit")]
    pub limit: u32,
}

pub fn default_edit_limit() -> u32 {
    5
}

/// Parameters for the `synrepo_change_impact` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ChangeImpactParams {
    /// Target file path or symbol name to assess change impact for.
    pub target: String,
}

pub fn handle_search(state: &SynrepoState, query: String, limit: u32) -> String {
    let result: anyhow::Result<serde_json::Value> = (|| {
        let matches = crate::substrate::search(&state.config, &state.repo_root, &query)?;

        let items: Vec<serde_json::Value> = matches
            .into_iter()
            .take(limit as usize)
            .map(|m| {
                json!({
                    "path": m.path.to_string_lossy(),
                    "line": m.line_number,
                    "content": String::from_utf8_lossy(&m.line_content).trim_end().to_string(),
                })
            })
            .collect();

        Ok(json!({ "query": query, "results": items }))
    })();
    render_result(result)
}

pub fn handle_overview(state: &SynrepoState) -> String {
    let result: anyhow::Result<serde_json::Value> = (|| {
        let synrepo_dir = crate::config::Config::synrepo_dir(&state.repo_root);
        let graph_dir = synrepo_dir.join("graph");
        let store = crate::store::sqlite::SqliteGraphStore::open_existing(&graph_dir)?;
        let stats = with_graph_snapshot(&store, || Ok(store.persisted_stats()?))?;
        Ok(json!({
            "mode": state.config.mode.to_string(),
            "graph": {
                "file_nodes": stats.file_nodes,
                "symbol_nodes": stats.symbol_nodes,
                "concept_nodes": stats.concept_nodes,
                "total_edges": stats.total_edges,
                "edges_by_kind": stats.edge_counts_by_kind,
            }
        }))
    })();
    render_result(result)
}

pub fn handle_where_to_edit(state: &SynrepoState, task: String, limit: u32) -> String {
    let result: anyhow::Result<serde_json::Value> = (|| {
        let matches = crate::substrate::search(&state.config, &state.repo_root, &task)?;

        let compiler = state
            .create_read_compiler()
            .map_err(|e| anyhow::anyhow!(e))?;
        let mut seen = HashSet::new();
        let mut cards = Vec::new();

        for m in &matches {
            let path = m.path.to_string_lossy().to_string();
            if seen.contains(&path) {
                continue;
            }
            seen.insert(path.clone());

            if let Some(file) = compiler.reader().file_by_path(&path)? {
                let card = compiler.file_card(file.id, Budget::Tiny)?;
                cards.push(serde_json::to_value(&card)?);
            }

            if cards.len() >= limit as usize {
                break;
            }
        }

        Ok(json!({ "task": task, "suggestions": cards }))
    })();
    render_result(result)
}

pub fn handle_change_impact(state: &SynrepoState, target: String) -> String {
    let result = (|| {
        let compiler = state
            .create_read_compiler()
            .map_err(|e| anyhow::anyhow!(e))?;
        let node_id = compiler
            .resolve_target(&target)?
            .ok_or_else(|| anyhow::anyhow!("target not found: {target}"))?;

        let imports_in = compiler
            .reader()
            .inbound(node_id, Some(EdgeKind::Imports))?;
        let calls_in = compiler.reader().inbound(node_id, Some(EdgeKind::Calls))?;

        let mut impacted_files: Vec<serde_json::Value> = Vec::new();
        let mut seen_files = HashSet::new();

        for edge in imports_in.iter().chain(calls_in.iter()) {
            let file_id = match edge.from {
                NodeId::File(id) => id,
                NodeId::Symbol(sym_id) => {
                    if let Some(sym) = compiler.reader().get_symbol(sym_id)? {
                        sym.file_id
                    } else {
                        continue;
                    }
                }
                _ => continue,
            };

            if seen_files.insert(file_id) {
                if let Some(file) = compiler.reader().get_file(file_id)? {
                    impacted_files.push(json!({
                        "path": file.path,
                        "edge_kind": edge.kind.as_str(),
                    }));
                }
            }
        }

        Ok(json!({
            "target": target,
            "impacted_files": impacted_files,
            "total": impacted_files.len(),
        }))
    })();
    render_result(result)
}
