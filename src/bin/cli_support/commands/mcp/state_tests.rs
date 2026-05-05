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
