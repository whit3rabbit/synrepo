use std::process::Command;

use synrepo::bootstrap::bootstrap;
use synrepo::config::Config;
use synrepo::pipeline::writer::{writer_lock_path, WriterOwnership};
use tempfile::tempdir;

#[test]
fn reconcile_completes_on_initialized_repo() {
    let repo = tempdir().unwrap();
    std::fs::create_dir_all(repo.path().join("src")).unwrap();
    std::fs::write(repo.path().join("src/lib.rs"), "pub fn greet() {}\n").unwrap();
    bootstrap(repo.path(), None).unwrap();

    super::super::commands::reconcile(repo.path()).unwrap();

    let synrepo_dir = synrepo::config::Config::synrepo_dir(repo.path());
    let state = synrepo::pipeline::watch::load_reconcile_state(&synrepo_dir)
        .expect("reconcile state must be written after reconcile");
    assert_eq!(state.last_outcome, "completed");
    assert!(
        state.files_discovered.unwrap_or(0) >= 1,
        "reconcile must discover at least src/lib.rs"
    );
}

#[test]
fn reconcile_refreshes_the_search_index() {
    let repo = tempdir().unwrap();
    std::fs::create_dir_all(repo.path().join("src")).unwrap();
    std::fs::write(
        repo.path().join("src/lib.rs"),
        "pub fn greet() { println!(\"old token\"); }\n",
    )
    .unwrap();
    bootstrap(repo.path(), None).unwrap();

    std::fs::write(
        repo.path().join("src/lib.rs"),
        "pub fn greet() { println!(\"new token\"); }\n",
    )
    .unwrap();

    super::super::commands::reconcile(repo.path()).unwrap();

    let config = Config::load(repo.path()).unwrap();
    let old_matches = synrepo::substrate::search(&config, repo.path(), "old token").unwrap();
    let new_matches = synrepo::substrate::search(&config, repo.path(), "new token").unwrap();

    assert!(old_matches.is_empty(), "stale index entry must be removed");
    assert_eq!(new_matches.len(), 1, "updated file must be searchable");
}

#[test]
fn reconcile_returns_lock_conflict_error_when_writer_busy() {
    let repo = tempdir().unwrap();
    std::fs::create_dir_all(repo.path().join("src")).unwrap();
    std::fs::write(repo.path().join("src/lib.rs"), "pub fn x() {}\n").unwrap();
    bootstrap(repo.path(), None).unwrap();

    let synrepo_dir = Config::synrepo_dir(repo.path());
    std::fs::create_dir_all(synrepo_dir.join("state")).unwrap();
    let mut child = Command::new("sleep").arg("5").spawn().unwrap();
    let pid = child.id();
    std::fs::write(
        writer_lock_path(&synrepo_dir),
        serde_json::to_string(&WriterOwnership {
            pid,
            acquired_at: "now".to_string(),
        })
        .unwrap(),
    )
    .unwrap();

    let err = super::super::commands::reconcile(repo.path()).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("writer lock") && msg.contains(&pid.to_string()),
        "expected lock-conflict error naming pid {pid}, got: {msg}"
    );

    let _ = child.kill();
    let _ = child.wait();
}
