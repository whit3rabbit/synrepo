use std::fs;

use tempfile::tempdir;

use crate::bootstrap::bootstrap;
use crate::config::Config;
use crate::surface::mcp::compact::OutputMode;
use crate::surface::mcp::SynrepoState;

use super::{default_limit, handle_search, SearchParams};

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
    }
}

#[test]
fn default_search_output_remains_compatible() {
    let (_dir, state) = make_state();
    let value: serde_json::Value = serde_json::from_str(&handle_search(
        &state,
        search_params(OutputMode::Default, None),
    ))
    .unwrap();

    assert_eq!(value["query"], "alpha");
    assert_eq!(value["engine"], "syntext");
    assert_eq!(value["source_store"], "substrate_index");
    assert!(value["results"]
        .as_array()
        .is_some_and(|rows| !rows.is_empty()));
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
