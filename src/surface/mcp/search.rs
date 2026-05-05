use std::process::Command;

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use syntext::SearchOptions;

use crate::core::ids::SymbolNodeId;

use super::compact::{self, OutputMode};
use super::helpers::render_result;
use super::limits::{
    bounded_limit_value, check_chars, DEFAULT_SEARCH_BUDGET_TOKENS, DEFAULT_SEARCH_LIMIT,
    MAX_SEARCH_CARDS_LIMIT, MAX_SEARCH_LIMIT, MAX_SEARCH_QUERY_CHARS,
};
use super::SynrepoState;

mod cards_mode;
mod impact;
mod overview;
mod where_to_edit;
use cards_mode::search_cards_response;
pub use impact::{
    handle_change_impact, handle_change_impact_with_direction, ChangeImpactParams, ImpactDirection,
};
pub use overview::{handle_degraded_overview, handle_overview};
pub use where_to_edit::handle_where_to_edit;

/// Parameters for the `synrepo_search` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchParams {
    pub repo_root: Option<std::path::PathBuf>,
    /// Lexical query string.
    pub query: String,
    /// Maximum number of results to return. Defaults to 10, capped at 50.
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
    /// Response shape. Defaults to compact routing output.
    #[serde(default)]
    pub output_mode: OutputMode,
    /// Optional numeric token cap for compact output. Defaults to 1500.
    #[serde(default)]
    pub budget_tokens: Option<usize>,
    /// Search mode. `auto` uses hybrid search when local semantic assets load.
    #[serde(default)]
    pub mode: SearchMode,
}

pub fn default_limit() -> u32 {
    DEFAULT_SEARCH_LIMIT as u32
}

/// Search strategy for MCP search.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, JsonSchema, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SearchMode {
    /// Hybrid when semantic triage is locally available, lexical otherwise.
    #[default]
    Auto,
    /// Exact syntext search only.
    Lexical,
}

impl SearchParams {
    fn search_options(&self) -> SearchOptions {
        let limit = self.effective_limit();
        SearchOptions {
            path_filter: self.path_filter.clone(),
            file_type: self.file_type.clone(),
            exclude_type: self.exclude_type.clone(),
            max_results: Some(limit),
            case_insensitive: self.case_insensitive,
        }
    }

    fn effective_limit(&self) -> usize {
        bounded_limit_value(self.limit as usize, DEFAULT_SEARCH_LIMIT, MAX_SEARCH_LIMIT)
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

/// Parameters for repository-scoped MCP tools that otherwise need no input.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct RepoRootParams {
    pub repo_root: Option<std::path::PathBuf>,
}

pub fn handle_search(state: &SynrepoState, params: SearchParams) -> String {
    let result: anyhow::Result<serde_json::Value> = (|| {
        check_chars("query", &params.query, MAX_SEARCH_QUERY_CHARS)?;
        let output_mode = params.output_mode;
        let effective_limit = params.effective_limit();
        if output_mode == OutputMode::Cards
            && effective_limit > MAX_SEARCH_CARDS_LIMIT
            && params.path_filter.is_none()
        {
            return Err(super::error::McpError::invalid_parameter(
                "output_mode=\"cards\" requires limit <= 5 or a path_filter; use compact output for broad routing",
            )
            .into());
        }
        let budget_tokens = params.budget_tokens.or(Some(DEFAULT_SEARCH_BUDGET_TOKENS));
        let options = params.search_options();
        let (items, engine, source_store, semantic_available) = match params.mode {
            SearchMode::Lexical => {
                let matches = crate::substrate::search_with_options(
                    &state.config,
                    &state.repo_root,
                    &params.query,
                    &options,
                )?;
                (lexical_items(matches), "syntext", "substrate_index", false)
            }
            SearchMode::Auto => {
                let report = crate::substrate::hybrid_search(
                    &state.config,
                    &state.repo_root,
                    &params.query,
                    &options,
                )?;
                let items = hybrid_items(state, report.rows);
                let source_store = if report.semantic_available {
                    "substrate_index+vector_index"
                } else {
                    "substrate_index"
                };
                (
                    items,
                    report.engine,
                    source_store,
                    report.semantic_available,
                )
            }
        };
        let result_count = items.len();
        let filters = params.filters_json();

        let response = json!({
            "query": params.query,
            "results": items,
            "engine": engine,
            "source_store": source_store,
            "mode": match params.mode {
                SearchMode::Auto => "auto",
                SearchMode::Lexical => "lexical",
            },
            "semantic_available": semantic_available,
            "limit": effective_limit,
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
            OutputMode::Cards => search_cards_response(state, &response, budget_tokens)?,
        })
    })();
    render_result(result)
}

fn lexical_items(matches: Vec<syntext::SearchMatch>) -> Vec<serde_json::Value> {
    matches
        .into_iter()
        .map(|m| {
            json!({
                "path": m.path.to_string_lossy(),
                "line": m.line_number,
                "content": String::from_utf8_lossy(&m.line_content).trim_end().to_string(),
                "source": "lexical",
                "fusion_score": serde_json::Value::Null,
                "semantic_score": serde_json::Value::Null,
            })
        })
        .collect()
}

fn hybrid_items(
    state: &SynrepoState,
    rows: Vec<crate::substrate::HybridSearchRow>,
) -> Vec<serde_json::Value> {
    let needs_graph = rows
        .iter()
        .any(|row| row.path.is_none() && row.symbol_id.is_some());
    if needs_graph {
        let fallback_rows = rows.clone();
        state
            .with_read_compiler(|compiler| Ok(hybrid_items_with_compiler(rows, Some(compiler))))
            .unwrap_or_else(|_| hybrid_items_with_compiler(fallback_rows, None))
    } else {
        hybrid_items_with_compiler(rows, None)
    }
}

fn hybrid_items_with_compiler(
    rows: Vec<crate::substrate::HybridSearchRow>,
    compiler: Option<&crate::surface::card::compiler::GraphCardCompiler>,
) -> Vec<serde_json::Value> {
    rows.into_iter()
        .map(|row| {
            let path = row
                .path
                .clone()
                .or_else(|| row.symbol_id.and_then(|id| symbol_path(compiler, id)));
            json!({
                "path": path,
                "line": row.line,
                "content": row.content,
                "source": row.source.as_str(),
                "fusion_score": row.fusion_score,
                "semantic_score": row.semantic_score,
                "chunk_id": row.chunk_id,
                "symbol_id": row.symbol_id,
            })
        })
        .collect()
}

fn symbol_path(
    compiler: Option<&crate::surface::card::compiler::GraphCardCompiler>,
    id: SymbolNodeId,
) -> Option<String> {
    let compiler = compiler?;
    let symbol = compiler.reader().get_symbol(id).ok().flatten()?;
    compiler
        .reader()
        .get_file(symbol.file_id)
        .ok()
        .flatten()
        .map(|file| file.path)
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
