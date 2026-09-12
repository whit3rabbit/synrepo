use std::fs;
use std::path::PathBuf;

use super::pending::manifest_path;
use super::*;
use crate::config::Config;
use tempfile::tempdir;

fn isolated_home() -> (tempfile::TempDir, crate::config::test_home::HomeEnvGuard) {
    let home = tempfile::tempdir().unwrap();
    let guard = crate::config::test_home::HomeEnvGuard::redirect_to(home.path());
    (home, guard)
}

fn bootstrap_repo() -> (
    tempfile::TempDir,
    Config,
    tempfile::TempDir,
    crate::config::test_home::HomeEnvGuard,
) {
    let (home, home_guard) = isolated_home();
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(repo.path().join("src/lib.rs"), "pub fn alpha() {}\n").unwrap();
    fs::create_dir_all(repo.path().join(".git")).unwrap();
    crate::bootstrap::bootstrap(repo.path(), None, false).unwrap();
    let config = Config::load(repo.path()).unwrap();
    (repo, config, home, home_guard)
}

#[test]
fn incremental_sync_makes_new_token_searchable() {
    let (repo, config, _home, _home_guard) = bootstrap_repo();
    fs::write(
        repo.path().join("src/lib.rs"),
        "pub fn alpha() {}\npub fn beta_token() {}\n",
    )
    .unwrap();

    let report =
        sync_index_incremental(&config, repo.path(), &[repo.path().join("src/lib.rs")]).unwrap();
    assert!(matches!(
        report.mode,
        IndexSyncMode::Incremental | IndexSyncMode::Rebuild
    ));
    let hits = crate::substrate::search(&config, repo.path(), "beta_token").unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].path, PathBuf::from("src/lib.rs"));
}

#[test]
fn incremental_sync_evicts_deleted_file() {
    let (repo, config, _home, _home_guard) = bootstrap_repo();
    fs::remove_file(repo.path().join("src/lib.rs")).unwrap();

    let report =
        sync_index_incremental(&config, repo.path(), &[repo.path().join("src/lib.rs")]).unwrap();
    assert_eq!(report.deleted_paths, 1);
    let hits = crate::substrate::search(&config, repo.path(), "alpha").unwrap();
    assert!(hits.is_empty());
}

#[test]
fn incremental_sync_skips_git_and_synrepo_runtime_paths() {
    let (repo, config, _home, _home_guard) = bootstrap_repo();
    fs::write(repo.path().join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
    fs::create_dir_all(repo.path().join(".synrepo/state")).unwrap();
    fs::write(repo.path().join(".synrepo/state/noise.txt"), "ignored").unwrap();

    let report = sync_index_incremental(
        &config,
        repo.path(),
        &[
            repo.path().join(".git/HEAD"),
            repo.path().join(".synrepo/state/noise.txt"),
        ],
    )
    .unwrap();
    assert_eq!(report.changed_paths, 0);
    assert_eq!(report.deleted_paths, 0);
    let hits = crate::substrate::search(&config, repo.path(), "alpha").unwrap();
    assert_eq!(hits.len(), 1);
}

#[test]
fn incremental_sync_refuses_redacted_files() {
    let (_home, _home_guard) = isolated_home();
    let repo = tempdir().unwrap();
    fs::write(repo.path().join("notes.txt"), "visible").unwrap();
    crate::bootstrap::bootstrap(repo.path(), None, false).unwrap();
    let mut config = Config::load(repo.path()).unwrap();
    config.redact_globs.push("**/*.secret".to_string());

    fs::write(repo.path().join("token.secret"), "hidden_value").unwrap();
    let report =
        sync_index_incremental(&config, repo.path(), &[repo.path().join("token.secret")]).unwrap();
    assert_eq!(report.deleted_paths, 1);
    let hits = crate::substrate::search(&config, repo.path(), "hidden_value").unwrap();
    assert!(hits.is_empty());
}

#[test]
fn incremental_sync_falls_back_to_rebuild_when_manifest_is_missing() {
    let (repo, config, _home, _home_guard) = bootstrap_repo();
    fs::remove_file(manifest_path(&config, repo.path())).unwrap();
    fs::write(
        repo.path().join("src/lib.rs"),
        "pub fn rebuilt_token() {}\n",
    )
    .unwrap();

    let report =
        sync_index_incremental(&config, repo.path(), &[repo.path().join("src/lib.rs")]).unwrap();
    assert_eq!(report.mode, IndexSyncMode::Rebuild);
    let hits = crate::substrate::search(&config, repo.path(), "rebuilt_token").unwrap();
    assert_eq!(hits.len(), 1);
}

#[test]
fn incremental_sync_is_durable_across_reopen() {
    let (repo, config, _home, _home_guard) = bootstrap_repo();
    let file_path = repo.path().join("src/lib.rs");

    // 1. Modify one file with a new token
    fs::write(&file_path, "pub fn durable_token_xyz() {}\n").unwrap();
    let report = sync_index_incremental(&config, repo.path(), &[file_path.clone()]).unwrap();
    assert!(report.durable);

    // Reopen in a fresh handle (search opens a new Index handle from disk)
    let hits = crate::substrate::search(&config, repo.path(), "durable_token_xyz").unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].path, PathBuf::from("src/lib.rs"));

    // 2. Delete the file and sync
    fs::remove_file(&file_path).unwrap();
    let del_report = sync_index_incremental(&config, repo.path(), &[file_path.clone()]).unwrap();
    assert!(del_report.durable);
    assert_eq!(del_report.deleted_paths, 1);

    // Reopen from disk again: deleted token must not be found
    let hits_after_del =
        crate::substrate::search(&config, repo.path(), "durable_token_xyz").unwrap();
    assert!(hits_after_del.is_empty());

    // 3. Recreate the file and sync
    fs::write(&file_path, "pub fn durable_token_xyz() {}\n").unwrap();
    let recreate_report =
        sync_index_incremental(&config, repo.path(), &[file_path.clone()]).unwrap();
    assert!(recreate_report.durable);

    // Reopen from disk again: recreated token must be found
    let hits_after_recreate =
        crate::substrate::search(&config, repo.path(), "durable_token_xyz").unwrap();
    assert_eq!(hits_after_recreate.len(), 1);
}
