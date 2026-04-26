use std::fs;

use super::super::commands::{ci_run_output, CiRunOptions};
use tempfile::tempdir;

#[test]
fn ci_run_uses_memory_store_without_initializing_repo() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(repo.path().join("src/lib.rs"), "pub fn ci_target() {}\n").unwrap();

    let out = ci_run_output(
        repo.path(),
        CiRunOptions {
            targets: vec!["src/lib.rs".to_string()],
            changed_from: None,
            budget: Some("tiny".to_string()),
            json: true,
        },
    )
    .unwrap();

    let json: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(json["store"], "memory");
    assert_eq!(json["cards"][0]["target_name"], "src/lib.rs");
    assert!(
        !repo.path().join(".synrepo").exists(),
        "ci-run must not create .synrepo"
    );
}

#[test]
fn ci_run_markdown_reports_unresolved_targets() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(repo.path().join("src/lib.rs"), "pub fn ci_target() {}\n").unwrap();

    let out = ci_run_output(
        repo.path(),
        CiRunOptions {
            targets: vec!["missing.rs".to_string()],
            changed_from: None,
            budget: None,
            json: false,
        },
    )
    .unwrap();

    assert!(out.contains("## synrepo CI Run"));
    assert!(out.contains("Unresolved: missing.rs"));
    assert!(out.contains("No risk cards produced."));
}
