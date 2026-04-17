#[cfg(unix)]
use std::process::Command;

use synrepo::bootstrap::bootstrap;
use synrepo::config::Config;
#[cfg(unix)]
use synrepo::pipeline::writer::{writer_lock_path, WriterOwnership};
use tempfile::tempdir;

#[cfg(feature = "semantic-triage")]
#[test]
fn init_with_semantic_triage_produces_vectors_index() {
    let repo = tempdir().unwrap();
    std::fs::create_dir_all(repo.path().join("src")).unwrap();
    std::fs::write(repo.path().join("src/lib.rs"), "pub fn greet() {}\n").unwrap();
    std::fs::create_dir_all(repo.path().join("docs/concepts")).unwrap();
    std::fs::write(
        repo.path().join("docs/concepts/test.md"),
        "# Test Concept\n\nA test concept for embedding.",
    )
    .unwrap();

    // Write config with semantic triage enabled
    let synrepo_dir = Config::synrepo_dir(repo.path());
    std::fs::create_dir_all(&synrepo_dir).unwrap();
    std::fs::write(
        synrepo_dir.join("config.toml"),
        "enable_semantic_triage = true\n",
    )
    .unwrap();

    bootstrap(repo.path(), None).unwrap();

    // Verify vectors directory exists with valid index
    let vectors_dir = synrepo_dir.join("index").join("vectors");
    assert!(
        vectors_dir.exists(),
        "vectors directory must exist when semantic_triage is enabled"
    );

    // Check for index file
    let index_path = vectors_dir.join("index.bin");
    assert!(
        index_path.exists(),
        "index.bin must exist in vectors directory"
    );
}

#[cfg(feature = "semantic-triage")]
#[test]
fn reconcile_rebuilds_vectors_index_after_deletion() {
    let repo = tempdir().unwrap();
    std::fs::create_dir_all(repo.path().join("src")).unwrap();
    std::fs::write(repo.path().join("src/lib.rs"), "pub fn greet() {}\n").unwrap();
    std::fs::create_dir_all(repo.path().join("docs/concepts")).unwrap();
    std::fs::write(
        repo.path().join("docs/concepts/test.md"),
        "# Test Concept\n\nA test concept.",
    )
    .unwrap();

    // Write config with semantic triage enabled
    let synrepo_dir = Config::synrepo_dir(repo.path());
    std::fs::create_dir_all(&synrepo_dir).unwrap();
    std::fs::write(
        synrepo_dir.join("config.toml"),
        "enable_semantic_triage = true\n",
    )
    .unwrap();

    bootstrap(repo.path(), None).unwrap();

    let vectors_dir = synrepo_dir.join("index").join("vectors");
    let index_path = vectors_dir.join("index.bin");

    // Verify initial index exists
    assert!(index_path.exists(), "initial index must exist");

    // Delete vectors directory
    std::fs::remove_dir_all(&vectors_dir).unwrap();
    assert!(!vectors_dir.exists(), "vectors directory must be deleted");

    // Run reconcile
    super::super::commands::reconcile(repo.path()).unwrap();

    // Verify index is rebuilt
    assert!(
        vectors_dir.exists(),
        "vectors directory must be rebuilt after reconcile"
    );
    assert!(
        index_path.exists(),
        "index.bin must be rebuilt after reconcile"
    );
}

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

#[cfg(unix)]
#[test]
fn reconcile_returns_lock_conflict_error_when_writer_busy() {
    use synrepo::pipeline::writer::hold_writer_flock_with_ownership;

    let repo = tempdir().unwrap();
    std::fs::create_dir_all(repo.path().join("src")).unwrap();
    std::fs::write(repo.path().join("src/lib.rs"), "pub fn x() {}\n").unwrap();
    bootstrap(repo.path(), None).unwrap();

    let synrepo_dir = Config::synrepo_dir(repo.path());
    std::fs::create_dir_all(synrepo_dir.join("state")).unwrap();
    let mut child = Command::new("sleep").arg("5").spawn().unwrap();
    let pid = child.id();
    let ownership = WriterOwnership {
        pid,
        acquired_at: "now".to_string(),
    };
    // Actually hold the kernel flock so reconcile's writer-lock acquire sees
    // a real conflict; the JSON is there only for display.
    let _flock = hold_writer_flock_with_ownership(&writer_lock_path(&synrepo_dir), &ownership);

    let err = super::super::commands::reconcile(repo.path()).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("writer lock") && msg.contains(&pid.to_string()),
        "expected lock-conflict error naming pid {pid}, got: {msg}"
    );

    let _ = child.kill();
    let _ = child.wait();
}
