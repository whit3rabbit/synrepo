use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::json;
use tempfile::{tempdir, TempDir};

use super::*;
use crate::cli_support::commands::mcp_runtime::prepare_state;
use synrepo::bootstrap::bootstrap;
use synrepo::config::{test_home, Config};
use synrepo::pipeline::watch::{watch_service_status, WatchServiceStatus};
use synrepo::store::sqlite::SqliteGraphStore;

struct HomeFixture {
    _lock: synrepo::test_support::GlobalTestLock,
    _home: TempDir,
    _guard: test_home::HomeEnvGuard,
}

fn home_fixture() -> HomeFixture {
    let lock = synrepo::test_support::global_test_lock(test_home::HOME_ENV_TEST_LOCK);
    let home = tempdir().unwrap();
    let guard = test_home::HomeEnvGuard::redirect_to(home.path());
    HomeFixture {
        _lock: lock,
        _home: home,
        _guard: guard,
    }
}

fn ready_repo(body: &str) -> (TempDir, PathBuf) {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/lib.rs"), body).unwrap();
    bootstrap(dir.path(), None, false).unwrap();
    let path = dir.path().to_path_buf();
    (dir, path)
}

fn resolve_error(result: anyhow::Result<Arc<SynrepoState>>) -> anyhow::Error {
    match result {
        Ok(_) => panic!("state resolution unexpectedly succeeded"),
        Err(error) => error,
    }
}

#[test]
fn default_repo_resolution_uses_prepared_default() {
    let _home = home_fixture();
    let (_repo, repo_path) = ready_repo("pub fn default_repo_needle() {}\n");
    let state = prepare_state(&repo_path).unwrap();
    let resolver = StateResolver::new(Some(state));

    let resolved = resolver.resolve(None).unwrap();
    assert_eq!(resolved.repo_root, repo_path);
}

#[test]
fn defaultless_missing_repo_root_is_loud() {
    let _home = home_fixture();
    let resolver = StateResolver::new(None);

    let err = resolve_error(resolver.resolve(None)).to_string();
    assert!(err.contains("repo_root is required"), "{err}");
}

#[test]
fn registered_repo_root_lazy_loads_and_routes_handlers() {
    let _home = home_fixture();
    let (_default_repo, default_path) = ready_repo("pub fn default_needle() {}\n");
    let (_target_repo, target_path) = ready_repo("pub fn target_needle_unique() {}\n");
    registry::record_project(&target_path).unwrap();
    let default_state = prepare_state(&default_path).unwrap();
    let resolver = StateResolver::new(Some(default_state));

    let resolved = resolver.resolve(Some(target_path.clone())).unwrap();
    let params =
        serde_json::from_value(json!({ "query": "target_needle_unique", "limit": 5 })).unwrap();
    let search_output = synrepo::surface::mcp::search::handle_search(&resolved, params);
    assert!(
        search_output.contains("target_needle_unique"),
        "{search_output}"
    );
    assert!(!search_output.contains("default_needle"), "{search_output}");

    let file_id = file_id_for(&target_path, "src/lib.rs");
    let node_output =
        synrepo::surface::mcp::primitives::handle_node(&resolved, file_id.to_string());
    assert!(
        node_output.contains("\"node_type\": \"file\""),
        "{node_output}"
    );
    assert!(!node_output.contains("\"error\""), "{node_output}");
}

#[test]
fn mcp_resolution_does_not_auto_start_watch() {
    let _home = home_fixture();
    let (_default_repo, default_path) = ready_repo("pub fn default_watch_free() {}\n");
    let (_target_repo, target_path) = ready_repo("pub fn target_watch_free() {}\n");
    registry::record_project(&target_path).unwrap();
    let default_state = prepare_state(&default_path).unwrap();
    let server = SynrepoServer::new_optional(Some(default_state), false);

    let default_watch = watch_service_status(&Config::synrepo_dir(&default_path));
    assert!(matches!(default_watch, WatchServiceStatus::Inactive));

    server.resolve_state(Some(target_path.clone())).unwrap();

    let target_watch = watch_service_status(&Config::synrepo_dir(&target_path));
    assert!(matches!(target_watch, WatchServiceStatus::Inactive));
}

#[test]
fn use_project_sets_default_for_defaultless_server() {
    let _home = home_fixture();
    let (_repo, repo_path) = ready_repo("pub fn project_default_needle() {}\n");
    registry::record_project(&repo_path).unwrap();
    let server = SynrepoServer::new_optional(None, false);

    let output = server.use_project(repo_path.clone());
    let value: serde_json::Value = serde_json::from_str(&output).unwrap();

    assert_eq!(value["status"], "default_set", "{output}");
    let resolved = server.resolve_state(None).unwrap();
    assert_eq!(
        resolved.repo_root,
        synrepo::registry::canonicalize_path(&repo_path)
    );
}

#[test]
fn metrics_json_reports_this_session_tool_counts() {
    let _home = home_fixture();
    let (_repo, repo_path) = ready_repo("pub fn metrics_needle() {}\n");
    let state = prepare_state(&repo_path).unwrap();
    let server = SynrepoServer::new_optional(Some(state), false);

    let output = server.with_tool_state("synrepo_overview", None, |state| {
        synrepo::surface::mcp::search::handle_overview(&state)
    });
    assert!(!output.contains("\"error\""), "{output}");

    let metrics = server.metrics_json(None);
    let value: serde_json::Value = serde_json::from_str(&metrics).unwrap();
    assert_eq!(
        value["this_session"]["calls_by_tool"]["synrepo_overview"],
        1
    );
    assert_eq!(value["this_session"]["errors_total"], 0);
}

#[test]
fn tool_state_clamps_large_outputs_and_records_budget_metrics() {
    let _home = home_fixture();
    let (_repo, repo_path) = ready_repo("pub fn clamp_metric_needle() {}\n");
    let state = prepare_state(&repo_path).unwrap();
    let server = SynrepoServer::new_optional(Some(state), false);

    let output = server.with_tool_state("synrepo_search", None, |_state| {
        let rows = (0..120)
            .map(|idx| json!({ "path": "src/lib.rs", "content": "x".repeat(300), "idx": idx }))
            .collect::<Vec<_>>();
        json!({ "results": rows }).to_string()
    });
    let value: serde_json::Value = serde_json::from_str(&output).unwrap();

    assert_eq!(value["context_accounting"]["truncation_applied"], true);
    let metrics = synrepo::pipeline::context_metrics::load(&Config::synrepo_dir(&repo_path))
        .expect("load context metrics");
    assert_eq!(metrics.responses_truncated_total, 1);
    assert_eq!(
        metrics.tool_token_totals.get("synrepo_search"),
        Some(&metrics.largest_response_tokens)
    );
}

#[test]
fn tool_state_records_error_codes_without_content() {
    let _home = home_fixture();
    let (_repo, repo_path) = ready_repo("pub fn error_code_metric_needle() {}\n");
    let state = prepare_state(&repo_path).unwrap();
    let server = SynrepoServer::new_optional(Some(state), false);

    let output = server.with_tool_state("synrepo_card", None, |_state| {
        json!({
            "ok": false,
            "error": {
                "code": "NOT_FOUND",
                "message": "target not found: private_target"
            },
            "error_message": "target not found: private_target"
        })
        .to_string()
    });
    assert!(response_has_error(&output), "{output}");

    let metrics = synrepo::pipeline::context_metrics::load(&Config::synrepo_dir(&repo_path))
        .expect("load context metrics");
    assert_eq!(metrics.mcp_tool_errors_total.get("synrepo_card"), Some(&1));
    let codes = metrics
        .mcp_tool_error_codes_total
        .get("synrepo_card")
        .unwrap();
    assert_eq!(codes.get("NOT_FOUND"), Some(&1));
    assert!(!serde_json::to_string(&metrics)
        .unwrap()
        .contains("private_target"));
}

#[test]
fn metrics_with_bad_explicit_repo_root_returns_error() {
    let _home = home_fixture();
    let dir = tempdir().unwrap();
    let server = SynrepoServer::new_optional(None, false);

    let output = server.metrics_for_repo_root(Some(dir.path().to_path_buf()));
    let value: serde_json::Value = serde_json::from_str(&output).unwrap();

    assert_eq!(value["ok"], false, "{output}");
    assert_eq!(value["error"]["code"], "NOT_FOUND", "{output}");
    assert_eq!(
        value["error_message"]
            .as_str()
            .is_some_and(|message| message.contains("not managed by synrepo")),
        true,
        "{output}"
    );
}

#[test]
fn default_tool_list_excludes_all_mutating_tools() {
    let server = SynrepoServer::new_optional(None, false);
    let names = server.registered_tool_names();

    for tool in [
        "synrepo_prepare_edit_context",
        "synrepo_apply_anchor_edits",
        "synrepo_refresh_commentary",
        "synrepo_note_add",
        "synrepo_note_link",
        "synrepo_note_supersede",
        "synrepo_note_forget",
        "synrepo_note_verify",
    ] {
        assert!(
            !names.iter().any(|name| name == tool),
            "{tool} was registered"
        );
    }
    assert!(names.iter().any(|name| name == "synrepo_readiness"));
    assert!(names.iter().any(|name| name == "synrepo_notes"));
    assert!(names.iter().any(|name| name == "synrepo_findings"));
    assert!(names.iter().any(|name| name == "synrepo_docs_search"));
}

#[test]
fn overlay_write_gate_exposes_overlay_writes_only() {
    let server = SynrepoServer::new_optional_with_overlay(None, true, false);
    let names = server.registered_tool_names();

    for tool in [
        "synrepo_refresh_commentary",
        "synrepo_note_add",
        "synrepo_note_link",
        "synrepo_note_supersede",
        "synrepo_note_forget",
        "synrepo_note_verify",
    ] {
        assert!(names.iter().any(|name| name == tool), "{tool} was hidden");
    }
    assert!(!names
        .iter()
        .any(|name| name == "synrepo_apply_anchor_edits"));
}

#[test]
fn source_edit_gate_exposes_source_edits_only() {
    let server = SynrepoServer::new_optional_with_overlay(None, false, true);
    let names = server.registered_tool_names();

    assert!(names
        .iter()
        .any(|name| name == "synrepo_prepare_edit_context"));
    assert!(names
        .iter()
        .any(|name| name == "synrepo_apply_anchor_edits"));
    assert!(!names.iter().any(|name| name == "synrepo_note_add"));
    assert!(!names
        .iter()
        .any(|name| name == "synrepo_refresh_commentary"));
}

#[test]
fn response_error_detection_uses_ok_false_only() {
    assert!(response_has_error(
        r#"{"ok":false,"error":{"code":"NOT_FOUND","message":"missing"}}"#
    ));
    assert!(!response_has_error(
        r#"{"error":"domain data, not a tool failure"}"#
    ));
    assert!(!response_has_error(r#"{"ok":true,"error":"domain data"}"#));
}

#[test]
fn blocking_tool_helper_returns_tool_output_under_tokio_runtime() {
    let _home = home_fixture();
    let (_repo, repo_path) = ready_repo("pub fn async_helper_needle() {}\n");
    let state = prepare_state(&repo_path).unwrap();
    let server = SynrepoServer::new_optional(Some(state), false);
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let output = runtime.block_on(server.with_tool_state_blocking(
        "synrepo_overview",
        None,
        |state| synrepo::surface::mcp::search::handle_overview(&state),
    ));
    let value: serde_json::Value = serde_json::from_str(&output).unwrap();

    assert!(value.get("error").is_none(), "{output}");
    assert!(
        value.get("mode").is_some() && value.get("graph").is_some(),
        "{output}"
    );
}

#[test]
fn persistent_tool_helper_does_not_return_timeout() {
    let _home = home_fixture();
    let (_repo, repo_path) = ready_repo("pub fn persistent_helper_needle() {}\n");
    let state = prepare_state(&repo_path).unwrap();
    let server = SynrepoServer::new_optional_with_timeout(
        Some(state),
        false,
        true,
        std::time::Duration::from_millis(1),
    );
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let output = runtime.block_on(server.with_tool_state_persistent(
        "synrepo_apply_anchor_edits",
        None,
        |_state| {
            std::thread::sleep(std::time::Duration::from_millis(20));
            serde_json::json!({ "ok": true, "status": "completed" }).to_string()
        },
    ));
    let value: serde_json::Value = serde_json::from_str(&output).unwrap();

    assert_eq!(value["status"], "completed", "{output}");
    assert_ne!(value["error"]["code"], "TIMEOUT", "{output}");
}

#[test]
fn unregistered_requested_repo_is_rejected() {
    let _home = home_fixture();
    let dir = tempdir().unwrap();
    let resolver = StateResolver::new(None);

    let err = resolve_error(resolver.resolve(Some(dir.path().to_path_buf()))).to_string();
    assert!(err.contains("not managed by synrepo"), "{err}");
    assert!(err.contains("synrepo project add"), "{err}");
}

#[test]
fn corrupt_registry_is_not_treated_as_empty() {
    let _home = home_fixture();
    let dir = tempdir().unwrap();
    let registry_path = registry::registry_path().unwrap();
    fs::create_dir_all(registry_path.parent().unwrap()).unwrap();
    fs::write(&registry_path, "not valid = @@@").unwrap();
    let resolver = StateResolver::new(None);

    let err = resolve_error(resolver.resolve(Some(dir.path().to_path_buf())));
    assert!(format!("{err:#}").contains("failed to parse registry"));
}

#[test]
fn requested_prepare_failure_does_not_fallback_to_default() {
    let _home = home_fixture();
    let (_default_repo, default_path) = ready_repo("pub fn default_only() {}\n");
    let bad_repo = tempdir().unwrap();
    registry::record_project(bad_repo.path()).unwrap();
    let default_state = prepare_state(&default_path).unwrap();
    let resolver = StateResolver::new(Some(default_state));

    let err = resolve_error(resolver.resolve(Some(bad_repo.path().to_path_buf()))).to_string();
    assert!(err.contains("failed to prepare"), "{err}");
    assert!(err.contains("synrepo init"), "{err}");
}

fn file_id_for(repo_root: &Path, path: &str) -> synrepo::FileNodeId {
    let graph_dir = Config::synrepo_dir(repo_root).join("graph");
    let store = SqliteGraphStore::open_existing(&graph_dir).unwrap();
    store.file_by_path(path).unwrap().unwrap().id
}
