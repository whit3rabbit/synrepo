use tempfile::tempdir;

use synrepo::bootstrap::bootstrap;
use synrepo::config::Config;
use synrepo::store::compatibility::snapshot_path;

use super::support::git;
// upgrade is re-exported at binary root scope via `use cli_support::commands::upgrade`
use crate::upgrade;

/// Minimal repo with a git init, a source file, and a completed bootstrap.
fn setup_bootstrapped_repo() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempdir().unwrap();
    let repo = dir.path();

    git(&dir, &["init", "-b", "main"]);
    git(&dir, &["config", "user.email", "test@example.com"]);
    git(&dir, &["config", "user.name", "Test"]);
    std::fs::write(repo.join("lib.rs"), "fn main() {}").unwrap();
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-m", "init"]);

    bootstrap(repo, None).unwrap();

    let repo_path = repo.to_path_buf();
    (dir, repo_path)
}

#[test]
fn upgrade_dry_run_does_not_mutate_stores() {
    let (dir, repo) = setup_bootstrapped_repo();
    let synrepo_dir = Config::synrepo_dir(&repo);
    let graph_db = synrepo_dir.join("graph").join("nodes.db");

    let mtime_before = graph_db.metadata().ok().and_then(|m| m.modified().ok());
    upgrade(&repo, false).expect("dry-run upgrade must succeed");
    let mtime_after = graph_db.metadata().ok().and_then(|m| m.modified().ok());

    assert_eq!(
        mtime_before, mtime_after,
        "dry-run must not modify the graph store"
    );
    drop(dir);
}

#[test]
fn upgrade_apply_on_current_runtime_exits_zero() {
    let (dir, repo) = setup_bootstrapped_repo();
    upgrade(&repo, true).expect("apply on current runtime must succeed with no changes");
    drop(dir);
}

#[test]
fn upgrade_apply_with_blocking_store_returns_error() {
    let (dir, repo) = setup_bootstrapped_repo();
    let synrepo_dir = Config::synrepo_dir(&repo);

    // Delete the compatibility snapshot while graph DB is materialized.
    // evaluate_runtime selects Block for the canonical graph store when a
    // materialized store has no snapshot.
    let snap = snapshot_path(&synrepo_dir);
    std::fs::remove_file(&snap).expect("snapshot must exist after bootstrap");

    let result = upgrade(&repo, true);
    assert!(
        result.is_err(),
        "apply with a blocking store must return an error"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("manual intervention") || msg.contains("blocked"),
        "error message must describe manual intervention, got: {msg}"
    );
    drop(dir);
}
