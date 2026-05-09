use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use syntext::SearchOptions;

use crate::surface::changed::git_changed_files;

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
mod rooted_rows;
mod where_to_edit;
use cards_mode::search_cards_response;
pub use impact::{
    handle_change_impact, handle_change_impact_with_direction, ChangeImpactParams, ImpactDirection,
};
pub use overview::{handle_degraded_overview, handle_orient, handle_overview};
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
    /// Treat the query as a literal string instead of a syntext/regex pattern.
    #[serde(default)]
    pub literal: bool,
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
            "literal": self.literal,
        })
    }
}

struct SearchExecution {
    items: Vec<Value>,
    engine: String,
    source_store: String,
    semantic_available: bool,
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
        let mut warnings = Vec::new();
        let mut pattern_mode = if params.literal { "literal" } else { "regex" };
        let mut effective_query = if params.literal {
            escape_search_pattern(&params.query)
        } else {
            params.query.clone()
        };
        let execution = match run_search(state, &params, &effective_query, &options) {
            Ok(execution) => execution,
            Err(error) if !params.literal && is_invalid_search_pattern(&error) => {
                pattern_mode = "literal_fallback";
                warnings.push(format!(
                    "query was not a valid search pattern and was retried as a literal string: {error}"
                ));
                effective_query = escape_search_pattern(&params.query);
                run_search(state, &params, &effective_query, &options)?
            }
            Err(error) => return Err(error),
        };
        let SearchExecution {
            items,
            engine,
            source_store,
            semantic_available,
        } = execution;
        let source_store = root_aware_source_store(&source_store, &items);
        let result_count = items.len();
        let filters = params.filters_json();

        let mut response = json!({
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
            "pattern_mode": pattern_mode,
        });
        if !warnings.is_empty() {
            if let Some(obj) = response.as_object_mut() {
                obj.insert("warnings".to_string(), json!(warnings));
            }
        }

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

fn run_search(
    state: &SynrepoState,
    params: &SearchParams,
    query: &str,
    options: &SearchOptions,
) -> anyhow::Result<SearchExecution> {
    Ok(match params.mode {
        SearchMode::Lexical => {
            let matches = crate::substrate::search_rooted_with_options(
                &state.config,
                &state.repo_root,
                query,
                options,
            )?;
            SearchExecution {
                items: rooted_rows::lexical_items(state, matches),
                engine: "syntext".to_string(),
                source_store: "substrate_index".to_string(),
                semantic_available: false,
            }
        }
        SearchMode::Auto => {
            let report =
                crate::substrate::hybrid_search(&state.config, &state.repo_root, query, options)?;
            let items = rooted_rows::hybrid_items(state, report.rows);
            let source_store = if report.semantic_available {
                "substrate_index+vector_index"
            } else {
                "substrate_index"
            };
            SearchExecution {
                items,
                engine: report.engine.to_string(),
                source_store: source_store.to_string(),
                semantic_available: report.semantic_available,
            }
        }
    })
}

fn is_invalid_search_pattern(error: &anyhow::Error) -> bool {
    let rendered = format!("{error:#}");
    rendered.contains("regex parse error") || rendered.contains("invalid pattern")
}

fn escape_search_pattern(query: &str) -> String {
    let mut escaped = String::with_capacity(query.len());
    for ch in query.chars() {
        if matches!(
            ch,
            '\\' | '.' | '+' | '*' | '?' | '(' | ')' | '|' | '[' | ']' | '{' | '}' | '^' | '$'
        ) {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped
}

fn root_aware_source_store(source_store: &str, items: &[Value]) -> &'static str {
    let has_non_primary = items.iter().any(|item| {
        item.get("is_primary_root")
            .and_then(Value::as_bool)
            .is_some_and(|is_primary| !is_primary)
    });
    match (source_store, has_non_primary) {
        ("substrate_index", true) => "substrate_index+direct_roots",
        ("substrate_index+vector_index", true) => "substrate_index+vector_index+direct_roots",
        ("substrate_index+direct_roots", _) => "substrate_index+direct_roots",
        ("substrate_index+vector_index+direct_roots", _) => {
            "substrate_index+vector_index+direct_roots"
        }
        ("substrate_index+vector_index", _) => "substrate_index+vector_index",
        _ => "substrate_index",
    }
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

#[cfg(test)]
mod tests;
