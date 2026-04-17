use std::fs;

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
fn report_reconcile_outcome_bails_on_failed() {
    use synrepo::pipeline::watch::ReconcileOutcome;
    let err = crate::report_reconcile_outcome(ReconcileOutcome::Failed("boom".into())).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("reconcile after rebuild failed") && msg.contains("boom"),
        "failure arm must surface both the prefix and the underlying cause, got: {msg}"
    );
}

#[test]
fn report_reconcile_outcome_bails_on_lock_conflict() {
    use synrepo::pipeline::watch::ReconcileOutcome;
    let err = crate::report_reconcile_outcome(ReconcileOutcome::LockConflict { holder_pid: 4242 })
        .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("writer lock is held by pid 4242"),
        "lock-conflict arm must name the holder, got: {msg}"
    );
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

/// `upgrade --apply` funnels through `acquire_write_admission` whenever it
/// has work to do, so a live watch-daemon lease must make it refuse rather
/// than reconciling in parallel with the watch service. This pins the
/// cross-command contention invariant for the upgrade path; the
/// export/sync counterparts live in tests/export.rs.
#[test]
fn upgrade_apply_blocked_when_watch_running() {
    let (dir, repo) = setup_bootstrapped_repo();
    let synrepo_dir = Config::synrepo_dir(&repo);

    // Force a Rebuild action so upgrade actually enters the admission gate;
    // same technique as upgrade_apply_rebuild_runs_reconcile_and_persists_completed_state.
    let updated = Config {
        roots: vec!["src".to_string()],
        ..Config::load(&repo).unwrap()
    };
    fs::write(
        synrepo_dir.join("config.toml"),
        toml::to_string_pretty(&updated).unwrap(),
    )
    .unwrap();

    // Plant a live watch-daemon lease. The current process PID is guaranteed
    // to be alive for the duration of the test, so ensure_watch_not_running
    // sees Running rather than Stale.
    let state_dir = synrepo_dir.join("state");
    fs::create_dir_all(&state_dir).unwrap();
    let state = serde_json::json!({
        "pid": std::process::id(),
        "started_at": "2026-01-01T00:00:00Z",
        "mode": "daemon",
        "socket_path": state_dir.join("watch.sock").display().to_string(),
    });
    fs::write(
        state_dir.join("watch-daemon.json"),
        serde_json::to_string(&state).unwrap(),
    )
    .unwrap();

    let err = upgrade(&repo, true).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("watch service is active"),
        "expected watch-active message from upgrade, got: {msg}"
    );
    drop(dir);
}

#[test]
fn upgrade_apply_rebuild_runs_reconcile_and_persists_completed_state() {
    use synrepo::config::Config;
    use synrepo::pipeline::watch::load_reconcile_state;

    let (dir, repo) = setup_bootstrapped_repo();
    let synrepo_dir = Config::synrepo_dir(&repo);

    // Change an index-sensitive config field so the snapshot fingerprint diverges
    // and evaluate_runtime returns Rebuild for the Index store. This drives the
    // `needs_reconcile` branch in upgrade.rs.
    let updated = Config {
        roots: vec!["src".to_string()],
        ..Config::load(&repo).unwrap()
    };
    std::fs::write(
        synrepo_dir.join("config.toml"),
        toml::to_string_pretty(&updated).unwrap(),
    )
    .unwrap();

    upgrade(&repo, true).expect("apply with Rebuild action must succeed");

    let state = load_reconcile_state(&synrepo_dir)
        .expect("reconcile-state.json must exist after rebuild-triggered reconcile");
    assert_eq!(
        state.last_outcome, "completed",
        "rebuild-triggered reconcile must complete, state: {state:?}"
    );

    drop(dir);
}
