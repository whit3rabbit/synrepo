use std::fs;
use tempfile::tempdir;

use super::support::init_empty_graph;
use crate::config::Config;
use crate::pipeline::export::{write_exports, ExportFormat};
use crate::surface::card::Budget;

#[test]
fn commit_flag_suppresses_gitignore_insertion() {
    let repo = tempdir().unwrap();
    let synrepo_dir = repo.path().join(".synrepo");
    init_empty_graph(&synrepo_dir).unwrap();

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
}

#[test]
fn no_commit_flag_inserts_gitignore_entry() {
    let repo = tempdir().unwrap();
    let synrepo_dir = repo.path().join(".synrepo");
    init_empty_graph(&synrepo_dir).unwrap();

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
}
