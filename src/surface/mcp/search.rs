use std::collections::HashSet;
use std::process::Command;

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use syntext::SearchOptions;

use crate::{
    core::ids::NodeId,
    structure::graph::EdgeKind,
    surface::card::CardCompiler,
};

use super::compact::{self, OutputMode};
use super::helpers::{render_result, with_graph_snapshot};
use super::SynrepoState;

mod where_to_edit;
pub use where_to_edit::handle_where_to_edit;

/// Parameters for the `synrepo_search` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchParams {
    pub repo_root: Option<std::path::PathBuf>,
    /// Lexical query string.
    pub query: String,
    /// Maximum number of results to return. Defaults to 20.
    #[serde(default = "default_limit")]
    pub limit: u32,
    /// Optional path prefix or glob filter.
    #[serde(default)]
    pub path_filter: Option<String>,
    /// Optional file extension/type to include (for example: "rs").
    #[serde(default)]
    pub file_type: Option<String>,
    /// Optional file extension/type to exclude (for example: "md").
    #[serde(default)]
    pub exclude_type: Option<String>,
    /// Whether matching should ignore ASCII case.
    #[serde(default, alias = "ignore_case")]
    pub case_insensitive: bool,
    /// Response shape. Defaults to the existing raw result list.
    #[serde(default)]
    pub output_mode: OutputMode,
    /// Optional numeric token cap for compact output.
    #[serde(default)]
    pub budget_tokens: Option<usize>,
}

pub fn default_limit() -> u32 {
    20
}

impl SearchParams {
    fn search_options(&self) -> SearchOptions {
        SearchOptions {
            path_filter: self.path_filter.clone(),
            file_type: self.file_type.clone(),
            exclude_type: self.exclude_type.clone(),
            max_results: Some(self.limit as usize),
            case_insensitive: self.case_insensitive,
        }
    }

    fn filters_json(&self) -> Value {
        json!({
            "path_filter": self.path_filter.clone(),
            "file_type": self.file_type.clone(),
            "exclude_type": self.exclude_type.clone(),
            "case_insensitive": self.case_insensitive,
        })
    }
}

/// Parameters for the `synrepo_where_to_edit` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct WhereToEditParams {
    pub repo_root: Option<std::path::PathBuf>,
    /// Plain-language description of the task (e.g. "add retry logic to HTTP client").
    pub task: String,
    /// Maximum number of file suggestions to return. Defaults to 5.
    #[serde(default = "default_edit_limit")]
    pub limit: u32,
    /// Optional numeric token cap for returned card suggestions.
    #[serde(default)]
    pub budget_tokens: Option<usize>,
}

pub fn default_edit_limit() -> u32 {
    5
}

/// Parameters for the `synrepo_change_impact` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ChangeImpactParams {
    pub repo_root: Option<std::path::PathBuf>,
    /// Target file path or symbol name to assess change impact for.
    pub target: String,
}

/// Parameters for repository-scoped MCP tools that otherwise need no input.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct RepoRootParams {
    pub repo_root: Option<std::path::PathBuf>,
}

pub fn handle_search(state: &SynrepoState, params: SearchParams) -> String {
    let result: anyhow::Result<serde_json::Value> = (|| {
        let output_mode = params.output_mode;
        let budget_tokens = params.budget_tokens;
        let options = params.search_options();
        let matches = crate::substrate::search_with_options(
            &state.config,
            &state.repo_root,
            &params.query,
            &options,
        )?;

        let items: Vec<serde_json::Value> = matches
            .into_iter()
            .map(|m| {
                json!({
                    "path": m.path.to_string_lossy(),
                    "line": m.line_number,
                    "content": String::from_utf8_lossy(&m.line_content).trim_end().to_string(),
                })
            })
            .collect();
        let result_count = items.len();
        let filters = params.filters_json();

        let response = json!({
            "query": params.query,
            "results": items,
            "engine": "syntext",
            "source_store": "substrate_index",
            "limit": params.limit,
            "filters": filters,
            "result_count": result_count,
        });

        Ok(match output_mode {
            OutputMode::Default => response,
            OutputMode::Compact => {
                let compacted = compact::compact_search_response(&response, budget_tokens);
                compact::record_output_accounting(state, &compacted);
                compacted
            }
        })
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

pub fn handle_changed(state: &SynrepoState) -> String {
    let result: anyhow::Result<serde_json::Value> = (|| {
        let changed_files = git_changed_files(&state.repo_root)?;
        let synrepo_dir = crate::config::Config::synrepo_dir(&state.repo_root);
        crate::pipeline::context_metrics::record_changed_files_best_effort(
            &synrepo_dir,
            changed_files.len(),
        );

        Ok(json!({
            "changed_files": changed_files,
            "changed_file_count": changed_files.len(),
            "index_state": if changed_files.is_empty() { "current_or_unknown" } else { "possibly_stale" },
            "recommended_commands": [
                "synrepo status",
                "synrepo check",
                "synrepo tests <changed-path>",
                "synrepo sync"
            ],
        }))
    })();
    render_result(result)
}

fn git_changed_files(repo_root: &std::path::Path) -> anyhow::Result<Vec<String>> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(repo_root)
        .output()?;
    if !output.status.success() {
        return Ok(vec![]);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut files = Vec::new();
    for line in stdout.lines() {
        if line.len() < 4 {
            continue;
        }
        let path = line[3..].trim();
        let path = path.rsplit(" -> ").next().unwrap_or(path);
        if !path.is_empty() {
            files.push(path.to_string());
        }
    }
    files.sort();
    files.dedup();
    Ok(files)
}

#[cfg(test)]
mod tests;
