use std::fs;

use tempfile::tempdir;

use crate::bootstrap::bootstrap;
use crate::config::Config;
use crate::surface::mcp::compact::OutputMode;
use crate::surface::mcp::SynrepoState;

use super::read_resource;
use super::{build_context_pack, ContextPackParams, ContextPackTarget};

fn make_state() -> (tempfile::TempDir, SynrepoState) {
    let home = tempdir().unwrap();
    let _home_guard = crate::config::test_home::HomeEnvGuard::redirect_to(home.path());
    let dir = tempdir().unwrap();
    let repo = dir.path();
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(
        repo.join("src/lib.rs"),
        "pub fn alpha() {}\npub fn beta() { alpha(); }\n",
    )
    .unwrap();
    fs::write(repo.join("src/other.rs"), "pub fn gamma() {}\n").unwrap();
    bootstrap(repo, None, false).unwrap();
    let state = SynrepoState {
        config: Config::load(repo).unwrap(),
        repo_root: repo.to_path_buf(),
    };
    (dir, state)
}

#[test]
fn context_pack_returns_file_outline_and_state() {
    let (_dir, state) = make_state();
    let value = build_context_pack(
        &state,
        ContextPackParams {
            repo_root: None,
            goal: Some("inspect lib".to_string()),
            targets: vec![ContextPackTarget {
                kind: "file".to_string(),
                target: "src/lib.rs".to_string(),
                budget: Some("normal".to_string()),
            }],
            budget: "tiny".to_string(),
            budget_tokens: None,
            output_mode: OutputMode::Default,
            include_tests: false,
            include_notes: false,
            limit: 8,
        },
    )
    .unwrap();

    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["artifacts"][0]["artifact_type"], "file_outline");
    assert_eq!(value["artifacts"][0]["content"]["path"], "src/lib.rs");
    assert!(value["context_state"]["source_hashes"]
        .as_array()
        .is_some_and(|hashes| !hashes.is_empty()));
}

#[test]
fn context_pack_rejects_invalid_budget() {
    let (_dir, state) = make_state();
    let err = build_context_pack(
        &state,
        ContextPackParams {
            repo_root: None,
            goal: None,
            targets: vec![ContextPackTarget {
                kind: "file".to_string(),
                target: "src/lib.rs".to_string(),
                budget: None,
            }],
            budget: "deeep".to_string(),
            budget_tokens: None,
            output_mode: OutputMode::Default,
            include_tests: false,
            include_notes: false,
            limit: 8,
        },
    )
    .unwrap_err();

    assert!(err.to_string().contains("invalid budget"), "{err}");
}

#[test]
fn context_pack_requires_targets_or_goal() {
    let (_dir, state) = make_state();
    let err = build_context_pack(
        &state,
        ContextPackParams {
            repo_root: None,
            goal: Some("   ".to_string()),
            targets: Vec::new(),
            budget: "tiny".to_string(),
            budget_tokens: None,
            output_mode: OutputMode::Default,
            include_tests: false,
            include_notes: false,
            limit: 8,
        },
    )
    .unwrap_err();

    assert!(err.to_string().contains("requires explicit targets"));
}

#[test]
fn context_pack_error_artifacts_are_typed() {
    let (_dir, state) = make_state();
    let value = build_context_pack(
        &state,
        ContextPackParams {
            repo_root: None,
            goal: None,
            targets: vec![ContextPackTarget {
                kind: "file".to_string(),
                target: "src/missing.rs".to_string(),
                budget: None,
            }],
            budget: "tiny".to_string(),
            budget_tokens: None,
            output_mode: OutputMode::Default,
            include_tests: false,
            include_notes: false,
            limit: 8,
        },
    )
    .unwrap();

    let artifact = &value["artifacts"][0];
    assert_eq!(artifact["artifact_type"], "error");
    assert_eq!(artifact["target_kind"], "file");
    assert_eq!(artifact["status"], "error");
    assert_eq!(artifact["severity"], "warning");
    assert_eq!(artifact["error"]["code"], "NOT_FOUND");
    assert_eq!(artifact["content"], serde_json::Value::Null);
}

#[test]
fn context_pack_preserves_order_and_omits_over_budget() {
    let (_dir, state) = make_state();
    let value = build_context_pack(
        &state,
        ContextPackParams {
            repo_root: None,
            goal: None,
            targets: vec![
                ContextPackTarget {
                    kind: "file".to_string(),
                    target: "src/lib.rs".to_string(),
                    budget: None,
                },
                ContextPackTarget {
                    kind: "file".to_string(),
                    target: "src/other.rs".to_string(),
                    budget: None,
                },
            ],
            budget: "tiny".to_string(),
            budget_tokens: Some(1),
            output_mode: OutputMode::Default,
            include_tests: false,
            include_notes: false,
            limit: 8,
        },
    )
    .unwrap();

    assert_eq!(value["artifacts"][0]["target"], "src/lib.rs");
    assert_eq!(value["omitted"][0]["target"], "src/other.rs");
    assert_eq!(value["context_state"]["truncation_applied"], true);
}

#[test]
fn context_pack_reports_targets_omitted_by_limit() {
    let (_dir, state) = make_state();
    let value = build_context_pack(
        &state,
        ContextPackParams {
            repo_root: None,
            goal: None,
            targets: vec![
                ContextPackTarget {
                    kind: "file".to_string(),
                    target: "src/lib.rs".to_string(),
                    budget: None,
                },
                ContextPackTarget {
                    kind: "file".to_string(),
                    target: "src/other.rs".to_string(),
                    budget: None,
                },
            ],
            budget: "tiny".to_string(),
            budget_tokens: None,
            output_mode: OutputMode::Default,
            include_tests: false,
            include_notes: false,
            limit: 1,
        },
    )
    .unwrap();

    assert_eq!(value["totals"]["limit"], 1);
    assert_eq!(value["omitted"][0]["target"], "src/other.rs");
    assert_eq!(value["omitted"][0]["reason"], "limit_reached");
}

#[test]
fn context_pack_metrics_record_once_per_retained_artifact() {
    let (_dir, state) = make_state();
    let synrepo_dir = Config::synrepo_dir(&state.repo_root);
    let _ = fs::remove_file(synrepo_dir.join("state").join("context-metrics.json"));

    let _value = build_context_pack(
        &state,
        ContextPackParams {
            repo_root: None,
            goal: None,
            targets: vec![ContextPackTarget {
                kind: "file".to_string(),
                target: "src/lib.rs".to_string(),
                budget: None,
            }],
            budget: "tiny".to_string(),
            budget_tokens: None,
            output_mode: OutputMode::Default,
            include_tests: false,
            include_notes: false,
            limit: 8,
        },
    )
    .unwrap();

    let metrics = crate::pipeline::context_metrics::load(&synrepo_dir).unwrap();
    assert_eq!(metrics.cards_served_total, 1);
}

#[test]
fn compact_context_pack_compacts_search_artifacts() {
    let (_dir, state) = make_state();
    let synrepo_dir = Config::synrepo_dir(&state.repo_root);
    let _ = fs::remove_file(synrepo_dir.join("state").join("context-metrics.json"));

    let value = build_context_pack(
        &state,
        ContextPackParams {
            repo_root: None,
            goal: None,
            targets: vec![ContextPackTarget {
                kind: "search".to_string(),
                target: "alpha".to_string(),
                budget: None,
            }],
            budget: "tiny".to_string(),
            budget_tokens: None,
            output_mode: OutputMode::Compact,
            include_tests: false,
            include_notes: false,
            limit: 8,
        },
    )
    .unwrap();

    let artifact = &value["artifacts"][0];
    assert_eq!(artifact["artifact_type"], "search");
    assert_eq!(artifact["content"]["output_mode"], "compact");
    assert!(artifact["content"]["file_groups"]
        .as_array()
        .is_some_and(|groups| !groups.is_empty()));
    assert!(artifact["content"]["output_accounting"].is_object());
    assert!(artifact["context_accounting"].is_object());

    let metrics = crate::pipeline::context_metrics::load(&synrepo_dir).unwrap();
    assert_eq!(metrics.compact_outputs_total, 1);
    assert_eq!(metrics.cards_served_total, 1);
}

#[test]
fn resource_context_pack_defaults_to_tiny_and_honors_budget_tokens() {
    let (_dir, state) = make_state();
    let output = read_resource(
        &state,
        "synrepo://context-pack?goal=alpha&limit=5&budget_tokens=200",
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_str(&output).unwrap();

    assert_eq!(value["context_state"]["budget_tier"], "tiny");
    assert_eq!(value["totals"]["token_cap"], 200);
}
