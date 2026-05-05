use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use tempfile::{tempdir, TempDir};

use super::*;
use synrepo::{bootstrap::bootstrap, config::test_home, registry};

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
fn use_project_rejects_unregistered_repo() {
    let _home = home_fixture();
    let (_repo, repo_path) = ready_repo("pub fn unregistered_project() {}\n");
    registry::remove_project(&repo_path).unwrap();
    let server = SynrepoServer::new_optional(None, false);

    let output = server.use_project(repo_path);
    let value: serde_json::Value = serde_json::from_str(&output).unwrap();

    assert_eq!(value["ok"], false, "{output}");
    assert_eq!(value["error"]["code"], "NOT_FOUND", "{output}");
    assert!(
        value["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("not managed by synrepo")),
        "{output}"
    );
}

#[test]
fn use_project_rejects_corrupt_registry() {
    let _home = home_fixture();
    let (_repo, repo_path) = ready_repo("pub fn corrupt_registry_project() {}\n");
    let registry_path = registry::registry_path().unwrap();
    fs::create_dir_all(registry_path.parent().unwrap()).unwrap();
    fs::write(&registry_path, "not valid = @@@").unwrap();
    let server = SynrepoServer::new_optional(None, false);

    let output = server.use_project(repo_path);
    let value: serde_json::Value = serde_json::from_str(&output).unwrap();

    assert_eq!(value["ok"], false, "{output}");
    assert!(
        value["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("failed to parse registry")),
        "{output}"
    );
}

#[test]
fn use_project_canonicalizes_root_before_setting_default() {
    let _home = home_fixture();
    let (_repo, repo_path) = ready_repo("pub fn canonical_project() {}\n");
    registry::record_project(&repo_path).unwrap();
    let alias = repo_path
        .join("..")
        .join(repo_path.file_name().expect("temp repo has file name"));
    let server = SynrepoServer::new_optional(None, false);

    let output = server.use_project(alias);
    let value: serde_json::Value = serde_json::from_str(&output).unwrap();

    assert_eq!(value["status"], "default_set", "{output}");
    let resolved = server.resolve_state(None).unwrap();
    assert_eq!(resolved.repo_root, registry::canonicalize_path(&repo_path));
}

#[test]
fn cached_project_is_revalidated_after_registry_removal() {
    let _home = home_fixture();
    let (_repo, repo_path) = ready_repo("pub fn removed_project() {}\n");
    registry::record_project(&repo_path).unwrap();
    let resolver = StateResolver::new(None);

    resolver.resolve(Some(repo_path.clone())).unwrap();
    registry::remove_project(&repo_path).unwrap();

    let err = resolve_error(resolver.resolve(Some(repo_path))).to_string();
    assert!(err.contains("not managed by synrepo"), "{err}");
}

#[test]
fn managed_default_is_revalidated_after_registry_removal() {
    let _home = home_fixture();
    let (_repo, repo_path) = ready_repo("pub fn removed_default_project() {}\n");
    registry::record_project(&repo_path).unwrap();
    let server = SynrepoServer::new_optional(None, false);

    let output = server.use_project(repo_path.clone());
    assert!(output.contains("default_set"), "{output}");
    registry::remove_project(&repo_path).unwrap();

    let err = resolve_error(server.resolve_state(None)).to_string();
    assert!(err.contains("not managed by synrepo"), "{err}");
}
