use std::fs;
use tempfile::tempdir;

use super::support::init_empty_graph;
use crate::config::Config;
use crate::pipeline::export::{write_exports, ExportFormat};
use crate::surface::card::Budget;

fn isolated_home() -> (
    tempfile::TempDir,
    crate::config::test_home::HomeEnvGuard,
    crate::test_support::GlobalTestLock,
) {
    let lock = crate::test_support::global_test_lock(crate::config::test_home::HOME_ENV_TEST_LOCK);
    let home = tempdir().unwrap();
    let guard = crate::config::test_home::HomeEnvGuard::redirect_to(home.path());
    (home, guard, lock)
}

#[test]
fn commit_flag_suppresses_gitignore_insertion() {
    let (_home, _guard, _lock) = isolated_home();
    let repo = tempdir().unwrap();
    let synrepo_dir = repo.path().join(".synrepo");
    init_empty_graph(&synrepo_dir).unwrap();
    crate::registry::record_install(repo.path(), false).unwrap();

    let config = Config {
        export_dir: "test-export-commit".to_string(),
        ..Config::default()
    };

    // With --commit, .gitignore should NOT be modified.
    write_exports(
        repo.path(),
        &synrepo_dir,
        &config,
        ExportFormat::Markdown,
        Budget::Normal,
        true,
    )
    .unwrap();

    assert!(
        !repo.path().join(".gitignore").exists(),
        ".gitignore should not be created when --commit is set"
    );
    let entry = crate::registry::get(repo.path()).unwrap().unwrap();
    assert!(!entry.export_gitignore_entry_added);
    assert!(entry.export_gitignore_entry.is_none());
}

#[test]
fn no_commit_flag_inserts_gitignore_entry() {
    let (_home, _guard, _lock) = isolated_home();
    let repo = tempdir().unwrap();
    let synrepo_dir = repo.path().join(".synrepo");
    init_empty_graph(&synrepo_dir).unwrap();
    crate::registry::record_install(repo.path(), false).unwrap();

    let config = Config {
        export_dir: "test-export-gitignore".to_string(),
        ..Config::default()
    };

    // Without --commit, .gitignore should contain the export dir entry.
    write_exports(
        repo.path(),
        &synrepo_dir,
        &config,
        ExportFormat::Markdown,
        Budget::Normal,
        false,
    )
    .unwrap();

    let gitignore = fs::read_to_string(repo.path().join(".gitignore")).unwrap();
    assert!(
        gitignore.contains("test-export-gitignore"),
        "gitignore should contain export dir: {gitignore}"
    );
    let entry = crate::registry::get(repo.path()).unwrap().unwrap();
    assert!(entry.export_gitignore_entry_added);
    assert_eq!(
        entry.export_gitignore_entry.as_deref(),
        Some("test-export-gitignore/")
    );
}
