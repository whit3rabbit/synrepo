use schemars::JsonSchema;
use serde::Deserialize;

use crate::substrate::discover_roots;
use crate::surface::refactor_suggestions::{
    collect_refactor_suggestions, RefactorSuggestionMode, RefactorSuggestionOptions, DEFAULT_LIMIT,
    DEFAULT_MIN_LINES,
};

use super::helpers::with_mcp_compiler;
use super::SynrepoState;

/// Parameters for the `synrepo_refactor_suggestions` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct RefactorSuggestionsParams {
    pub repo_root: Option<std::path::PathBuf>,
    /// Suggestion mode. Defaults to `line_count`.
    #[serde(default)]
    pub mode: RefactorSuggestionMode,
    /// Physical-line threshold. Files must be greater than this value.
    #[serde(default = "default_min_lines")]
    pub min_lines: usize,
    /// Maximum number of candidates to return. Defaults to 20.
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Optional path prefix or glob filter.
    #[serde(default)]
    pub path_filter: Option<String>,
}

fn default_min_lines() -> usize {
    DEFAULT_MIN_LINES
}

fn default_limit() -> usize {
    DEFAULT_LIMIT
}

pub fn handle_refactor_suggestions(
    state: &SynrepoState,
    params: RefactorSuggestionsParams,
) -> String {
    with_mcp_compiler(state, |compiler| {
        let roots = discover_roots(&state.repo_root, &state.config);
        Ok(collect_refactor_suggestions(
            compiler,
            &roots,
            RefactorSuggestionOptions {
                mode: params.mode,
                min_lines: params.min_lines,
                limit: params.limit,
                path_filter: params.path_filter,
            },
        )?)
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;
    use crate::config::Config;

    fn make_state() -> (tempfile::TempDir, SynrepoState) {
        let home = tempdir().unwrap();
        let _home_guard = crate::config::test_home::HomeEnvGuard::redirect_to(home.path());
        let dir = tempdir().unwrap();
        let repo = dir.path();
        fs::create_dir_all(repo.join("src")).unwrap();
        fs::write(repo.join("src/lib.rs"), rust_lines(320, "large_lib")).unwrap();
        fs::write(repo.join("src/tests.rs"), rust_lines(500, "ignored_tests")).unwrap();
        crate::bootstrap::bootstrap(repo, None, false).unwrap();
        let state = SynrepoState {
            config: Config::load(repo).unwrap(),
            repo_root: repo.to_path_buf(),
        };
        (dir, state)
    }

    fn rust_lines(lines: usize, name: &str) -> String {
        let mut body = format!("pub fn {name}() {{}}\n");
        for idx in 1..lines {
            body.push_str(&format!("// {name} {idx}\n"));
        }
        body
    }

    #[test]
    fn handler_returns_large_file_suggestion_contract() {
        let (_dir, state) = make_state();
        let output = handle_refactor_suggestions(
            &state,
            RefactorSuggestionsParams {
                repo_root: None,
                mode: RefactorSuggestionMode::LineCount,
                min_lines: 300,
                limit: 20,
                path_filter: None,
            },
        );
        let value: serde_json::Value = serde_json::from_str(&output).unwrap();

        assert_eq!(value["source_store"], "graph+filesystem");
        assert_eq!(value["mode"], "line_count");
        assert_eq!(value["metric"], "physical_lines");
        assert_eq!(value["threshold"], 300);
        assert_eq!(value["criteria"]["line_count_threshold"], 300);
        assert_eq!(value["criteria"]["line_count_threshold_applied"], true);
        assert_eq!(value["candidate_count"], 1);
        assert_eq!(value["candidates"][0]["path"], "src/lib.rs");
        assert_eq!(value["candidates"][0]["language"], "rust");
        assert!(value["candidates"][0]["recommended_follow_up"]
            .as_array()
            .is_some_and(|items| !items.is_empty()));
    }

    #[test]
    fn handler_returns_missing_docs_contract() {
        let (_dir, state) = make_state();
        let output = handle_refactor_suggestions(
            &state,
            RefactorSuggestionsParams {
                repo_root: None,
                mode: RefactorSuggestionMode::MissingDocs,
                min_lines: 300,
                limit: 20,
                path_filter: None,
            },
        );
        let value: serde_json::Value = serde_json::from_str(&output).unwrap();

        assert_eq!(value["source_store"], "graph+filesystem");
        assert_eq!(value["mode"], "missing_docs");
        assert_eq!(value["metric"], "missing_public_docs");
        assert_eq!(value["threshold"], 300);
        assert_eq!(value["criteria"]["line_count_threshold"], 300);
        assert_eq!(value["criteria"]["line_count_threshold_applied"], false);
        assert_eq!(value["criteria"]["visibility"], "public");
        assert_eq!(value["criteria"]["doc_source"], "ast_doc_comment");
        assert_eq!(value["candidate_count"], 1);
        assert_eq!(value["candidates"][0]["path"], "src/lib.rs");
        assert_eq!(value["candidates"][0]["missing_public_doc_count"], 1);
        assert!(
            value["candidates"][0]["missing_public_docs"][0]["symbol_id"]
                .as_str()
                .is_some_and(|id| id.starts_with("sym_"))
        );
        assert!(value["candidates"][0]["missing_public_docs"]
            .as_array()
            .is_some_and(|items| !items.is_empty()));
    }
}
