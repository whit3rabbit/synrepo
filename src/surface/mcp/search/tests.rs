use std::fs;

use tempfile::tempdir;

use crate::bootstrap::bootstrap;
use crate::config::Config;
use crate::surface::mcp::compact::OutputMode;
use crate::surface::mcp::SynrepoState;

use super::{default_edit_limit, default_limit, handle_search, handle_where_to_edit, SearchParams};

fn make_state() -> (tempfile::TempDir, SynrepoState) {
    let home = tempdir().unwrap();
    let _home_guard = crate::config::test_home::HomeEnvGuard::redirect_to(home.path());
    let dir = tempdir().unwrap();
    let repo = dir.path();
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(
        repo.join("src/lib.rs"),
        "pub fn alpha() {}\nalpha one\nalpha two\nalpha three\nalpha four\nalpha five\nalpha six\n",
    )
    .unwrap();
    fs::write(
        repo.join("src/other.rs"),
        "// distinct\npub fn alpha_other() {}\n",
    )
    .unwrap();
    bootstrap(repo, None, false).unwrap();
    let state = SynrepoState {
        config: Config::load(repo).unwrap(),
        repo_root: repo.to_path_buf(),
    };
    (dir, state)
}

fn search_params(output_mode: OutputMode, budget_tokens: Option<usize>) -> SearchParams {
    SearchParams {
        repo_root: None,
        query: "alpha".to_string(),
        limit: default_limit(),
        path_filter: None,
        file_type: None,
        exclude_type: None,
        case_insensitive: false,
        output_mode,
        budget_tokens,
        mode: super::SearchMode::Auto,
    }
}

#[test]
fn search_defaults_to_compact_and_bounded() {
    let (_dir, state) = make_state();
    let params: SearchParams = serde_json::from_value(serde_json::json!({
        "query": "alpha",
        "limit": 0
    }))
    .unwrap();
    let value: serde_json::Value = serde_json::from_str(&handle_search(&state, params)).unwrap();

    assert_eq!(value["query"], "alpha");
    assert_eq!(value["engine"], "syntext");
    assert_eq!(value["source_store"], "substrate_index");
    assert_eq!(value["mode"], "auto");
    assert_eq!(value["semantic_available"], false);
    assert_eq!(value["output_mode"], "compact");
    assert_eq!(value["limit"], 1);
    assert!(value.get("results").is_none());
    assert!(value["output_accounting"].is_object());
}

#[test]
fn explicit_default_search_output_remains_available() {
    let (_dir, state) = make_state();
    let mut params = search_params(OutputMode::Default, None);
    params.limit = 1000;
    let value: serde_json::Value = serde_json::from_str(&handle_search(&state, params)).unwrap();

    assert_eq!(value["query"], "alpha");
    assert!(value["results"]
        .as_array()
        .is_some_and(|rows| !rows.is_empty()));
    assert_eq!(value["limit"], 50);
    assert!(value.get("output_accounting").is_none());
}

#[test]
fn compact_search_groups_results_and_suggests_cards() {
    let (_dir, state) = make_state();
    let value: serde_json::Value = serde_json::from_str(&handle_search(
        &state,
        search_params(OutputMode::Compact, None),
    ))
    .unwrap();

    assert_eq!(value["output_mode"], "compact");
    assert!(value.get("results").is_none());
    assert!(value["file_groups"]
        .as_array()
        .is_some_and(|groups| !groups.is_empty()));
    assert!(value["suggested_card_targets"]
        .as_array()
        .is_some_and(|targets| targets.iter().any(|t| t == "src/lib.rs")));
    assert!(value["output_accounting"]["returned_token_estimate"]
        .as_u64()
        .is_some_and(|tokens| tokens > 0));
}

#[test]
fn compact_search_budget_reports_omissions() {
    let (_dir, state) = make_state();
    let value: serde_json::Value = serde_json::from_str(&handle_search(
        &state,
        search_params(OutputMode::Compact, Some(1)),
    ))
    .unwrap();

    assert_eq!(value["output_accounting"]["truncation_applied"], true);
    assert!(value["output_accounting"]["omitted_count"]
        .as_u64()
        .is_some_and(|count| count > 0));
    assert!(value["file_groups"]
        .as_array()
        .is_some_and(|groups| groups.len() == 1));
    assert!(value["suggested_card_targets"]
        .as_array()
        .is_some_and(|targets| targets.len() == 1));
}

#[test]
fn cards_search_returns_deduped_tiny_file_cards() {
    let (_dir, state) = make_state();
    let mut params = search_params(OutputMode::Cards, None);
    params.query = "pub fn alpha".to_string();
    params.limit = 5;
    let value: serde_json::Value = serde_json::from_str(&handle_search(&state, params)).unwrap();

    assert_eq!(value["output_mode"], "cards");
    assert_eq!(value["source_store"], "graph");
    assert_eq!(value["search_source_store"], "substrate_index");
    let cards = value["cards"].as_array().unwrap();
    assert_eq!(cards.len(), 2, "{value}");
    assert!(cards
        .iter()
        .any(|card| card["path"].as_str() == Some("src/lib.rs")));
    assert!(value["unresolved"].as_array().unwrap().is_empty());
}

#[test]
fn cards_search_rejects_broad_queries() {
    let (_dir, state) = make_state();
    let value: serde_json::Value = serde_json::from_str(&handle_search(
        &state,
        search_params(OutputMode::Cards, None),
    ))
    .unwrap();

    assert_eq!(value["ok"], false);
    assert_eq!(value["error"]["code"], "INVALID_PARAMETER");
}

#[test]
fn compact_search_metrics_do_not_store_content() {
    let (_dir, state) = make_state();
    let synrepo_dir = Config::synrepo_dir(&state.repo_root);
    let _ = fs::remove_file(synrepo_dir.join("state").join("context-metrics.json"));

    let _ = handle_search(&state, search_params(OutputMode::Compact, None));

    let metrics = crate::pipeline::context_metrics::load(&synrepo_dir).unwrap();
    assert_eq!(metrics.compact_outputs_total, 1);
    assert!(metrics.compact_returned_tokens_total > 0);
    let serialized = serde_json::to_string(&metrics).unwrap();
    assert!(!serialized.contains("alpha"));
    assert!(!serialized.contains("src/lib.rs"));
}

fn make_routing_state() -> (tempfile::TempDir, SynrepoState) {
    let home = tempdir().unwrap();
    let _home_guard = crate::config::test_home::HomeEnvGuard::redirect_to(home.path());
    let dir = tempdir().unwrap();
    let repo = dir.path();
    fs::create_dir_all(repo.join("src/bin/cli_support/commands/agent_hooks")).unwrap();
    fs::create_dir_all(repo.join("src/pipeline/context_metrics")).unwrap();
    fs::write(
        repo.join("src/bin/cli_support/commands/agent_hooks/classify.rs"),
        "pub fn classify_hook_nudge() {}\n// agent_hooks nudge classifier routing\n",
    )
    .unwrap();
    fs::write(
        repo.join("src/bin/cli_support/commands/agent_hooks/render.rs"),
        "pub fn render_hook_nudge() {}\n// agent_hooks nudge renderer signals\n",
    )
    .unwrap();
    fs::write(
        repo.join("src/pipeline/context_metrics/mod.rs"),
        "pub fn record_context_metric() {}\n// context_metrics budget accounting\n",
    )
    .unwrap();
    bootstrap(repo, None, false).unwrap();
    let state = SynrepoState {
        config: Config::load(repo).unwrap(),
        repo_root: repo.to_path_buf(),
    };
    (dir, state)
}

#[test]
fn where_to_edit_decomposes_broad_task_into_snake_case_anchors() {
    let (_dir, state) = make_routing_state();
    let value: serde_json::Value = serde_json::from_str(&handle_where_to_edit(
        &state,
        "agent hook routing recommendations with context metrics".to_string(),
        default_edit_limit(),
        None,
    ))
    .unwrap();

    assert_eq!(value["fallback_used"], true);
    assert_eq!(value["miss_reason"], serde_json::Value::Null);
    let paths = suggestion_paths(&value);
    assert!(paths
        .iter()
        .any(|path| path.ends_with("agent_hooks/classify.rs")));
    assert!(paths
        .iter()
        .any(|path| path.ends_with("context_metrics/mod.rs")));
    let attempts = value["query_attempts"].as_array().unwrap();
    assert!(attempts
        .iter()
        .any(|attempt| attempt["query"] == "agent_hooks"));
    assert!(attempts
        .iter()
        .any(|attempt| attempt["query"] == "context_metrics"));
}

#[test]
fn where_to_edit_handles_observed_agent_hook_miss_phrase() {
    let (_dir, state) = make_routing_state();
    let value: serde_json::Value = serde_json::from_str(&handle_where_to_edit(
        &state,
        "extend agent hook nudge classifier renderer with structured fast path signals".to_string(),
        default_edit_limit(),
        None,
    ))
    .unwrap();

    assert_eq!(value["fallback_used"], true);
    assert_eq!(value["miss_reason"], serde_json::Value::Null);
    let paths = suggestion_paths(&value);
    assert!(paths
        .iter()
        .any(|path| path.ends_with("agent_hooks/classify.rs")));
    assert!(paths
        .iter()
        .any(|path| path.ends_with("agent_hooks/render.rs")));
}

#[test]
fn where_to_edit_reports_zero_match_diagnostics_without_fake_suggestions() {
    let (_dir, state) = make_routing_state();
    let value: serde_json::Value = serde_json::from_str(&handle_where_to_edit(
        &state,
        "definitely absent qqqqq route".to_string(),
        default_edit_limit(),
        None,
    ))
    .unwrap();

    assert_eq!(value["fallback_used"], true);
    assert_eq!(value["miss_reason"], "no_index_matches");
    assert!(value["suggestions"].as_array().unwrap().is_empty());
    assert!(value["query_attempts"]
        .as_array()
        .is_some_and(|attempts| !attempts.is_empty()));
    assert_eq!(value["recommended_tool"], serde_json::Value::Null);
    assert!(value["recommended_next_queries"]
        .as_array()
        .is_some_and(|queries| queries.is_empty()));
}

#[test]
fn where_to_edit_recommends_exact_queries_on_mcp_review_miss() {
    let (_dir, state) = make_routing_state();
    let value: serde_json::Value = serde_json::from_str(&handle_where_to_edit(
        &state,
        "review MCP mutability and parameter validation".to_string(),
        default_edit_limit(),
        None,
    ))
    .unwrap();

    assert_eq!(value["miss_reason"], "no_index_matches");
    assert_eq!(value["recommended_tool"], "synrepo_search");
    let queries = value["recommended_next_queries"].as_array().unwrap();
    assert!(queries
        .iter()
        .any(|q| q.as_str() == Some("name = \"synrepo_")));
    assert!(queries
        .iter()
        .any(|q| q.as_str() == Some("allow-source-edits")));
    assert!(queries.iter().any(|q| q.as_str() == Some("parse_budget")));
}

fn suggestion_paths(value: &serde_json::Value) -> Vec<String> {
    value["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|card| card["path"].as_str().map(ToOwned::to_owned))
        .collect()
}
