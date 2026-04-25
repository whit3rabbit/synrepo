use synrepo::bootstrap::bootstrap;
use synrepo::config::Config;
use synrepo::pipeline::structural::CompileSummary;
use synrepo::pipeline::watch::{persist_reconcile_state, ReconcileOutcome};
use synrepo::pipeline::writer::{writer_lock_path, WriterOwnership};
use tempfile::tempdir;

use super::{seed_graph, status_output};

#[test]
fn status_next_step_routes_to_unknown_reconcile() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());

    let text = status_output(repo.path(), false, false, false).unwrap();
    assert!(
        text.contains("next step:    run `synrepo reconcile` to do the first graph pass"),
        "expected first-reconcile next-step line, got: {text}"
    );
}

#[test]
#[cfg(unix)]
fn status_next_step_routes_to_writer_lock_when_held() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());

    let synrepo_dir = Config::synrepo_dir(repo.path());
    // Take the kernel flock so `compute_writer_status` actually observes
    // contention; stamping JSON alone leaves the lock free. See CLAUDE.md
    // writer-lock gotcha.
    let (mut child, pid) = synrepo::pipeline::writer::live_foreign_pid();
    let _flock = synrepo::pipeline::writer::hold_writer_flock_with_ownership(
        &writer_lock_path(&synrepo_dir),
        &WriterOwnership {
            pid,
            acquired_at: "now".to_string(),
        },
    );

    let text = status_output(repo.path(), false, false, false).unwrap();
    assert!(
        text.contains("writer lock is held"),
        "expected writer-lock-held next-step line, got: {text}"
    );

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn status_next_step_routes_to_current_when_reconcile_completed() {
    let repo = tempdir().unwrap();
    bootstrap(repo.path(), None, false).unwrap();
    let synrepo_dir = Config::synrepo_dir(repo.path());

    persist_reconcile_state(
        &synrepo_dir,
        &ReconcileOutcome::Completed(CompileSummary::default()),
        0,
    );

    let text = status_output(repo.path(), false, false, false).unwrap();
    assert!(
        text.contains("graph is current"),
        "expected `graph is current` next-step line, got: {text}"
    );
}

#[test]
fn status_reports_corrupt_reconcile_state() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());
    let synrepo_dir = Config::synrepo_dir(repo.path());
    let state_dir = synrepo_dir.join("state");
    std::fs::create_dir_all(&state_dir).unwrap();
    std::fs::write(state_dir.join("reconcile-state.json"), b"not valid json").unwrap();

    let text = status_output(repo.path(), false, false, false).unwrap();
    assert!(text.contains("reconcile:    corrupt"), "got: {text}");

    let json: serde_json::Value = serde_json::from_str(
        status_output(repo.path(), true, false, false)
            .unwrap()
            .trim(),
    )
    .unwrap();
    assert_eq!(json["reconcile_health"], "corrupt");
}

#[test]
fn status_reports_corrupt_writer_lock() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());
    let synrepo_dir = Config::synrepo_dir(repo.path());
    let state_dir = synrepo_dir.join("state");
    std::fs::create_dir_all(&state_dir).unwrap();
    std::fs::write(state_dir.join("writer.lock"), b"not valid json").unwrap();

    let text = status_output(repo.path(), false, false, false).unwrap();
    assert!(text.contains("writer lock:  corrupt"), "got: {text}");

    let json: serde_json::Value = serde_json::from_str(
        status_output(repo.path(), true, false, false)
            .unwrap()
            .trim(),
    )
    .unwrap();
    assert_eq!(json["writer_lock"], "corrupt");
}

#[test]
fn status_reports_corrupt_watch_state() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());
    let synrepo_dir = Config::synrepo_dir(repo.path());
    let state_dir = synrepo_dir.join("state");
    std::fs::create_dir_all(&state_dir).unwrap();
    std::fs::write(state_dir.join("watch-daemon.json"), b"not valid json").unwrap();

    let text = status_output(repo.path(), false, false, false).unwrap();
    assert!(text.contains("watch:        corrupt"), "got: {text}");

    let json: serde_json::Value = serde_json::from_str(
        status_output(repo.path(), true, false, false)
            .unwrap()
            .trim(),
    )
    .unwrap();
    assert!(json["watch"].as_str().unwrap().contains("corrupt"));
}
