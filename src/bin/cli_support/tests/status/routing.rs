use synrepo::bootstrap::bootstrap;
use synrepo::config::Config;
use synrepo::pipeline::context_metrics::{self, ContextMetrics};
use synrepo::pipeline::structural::CompileSummary;
use synrepo::pipeline::watch::{persist_reconcile_state, ReconcileOutcome};
use synrepo::pipeline::writer::{writer_lock_path, WriterOwnership};
use tempfile::tempdir;

use super::{seed_graph, status_output};

#[test]
fn status_next_step_routes_to_unknown_reconcile() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());

    // Bootstrap now persists reconcile-state.json itself (structural compile
    // is the first reconcile pass). To exercise the `ReconcileHealth::Unknown`
    // → first-reconcile next-step routing, simulate the recoverable case
    // where the state file is absent (hand-deleted or pre-fix runtime).
    let state_path = Config::synrepo_dir(repo.path())
        .join("state")
        .join("reconcile-state.json");
    if state_path.exists() {
        std::fs::remove_file(&state_path).unwrap();
    }

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

#[test]
fn status_surfaces_fast_path_context_metrics() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());
    let synrepo_dir = Config::synrepo_dir(repo.path());
    let metrics = ContextMetrics {
        route_classifications_total: 3,
        context_fast_path_signals_total: 2,
        deterministic_edit_candidates_total: 1,
        anchored_edit_accepted_total: 4,
        anchored_edit_rejected_total: 1,
        estimated_llm_calls_avoided_total: 2,
        ..ContextMetrics::default()
    };
    context_metrics::save(&synrepo_dir, &metrics).unwrap();

    let text = status_output(repo.path(), false, false, false).unwrap();
    assert!(
        text.contains("fast path:  3 route(s), 2 context signal(s), 1 edit candidate(s), 2 est. LLM call(s) avoided"),
        "got: {text}"
    );
    assert!(
        text.contains("anchors:    4 accepted edit(s), 1 rejected edit(s)"),
        "got: {text}"
    );

    let json: serde_json::Value = serde_json::from_str(
        status_output(repo.path(), true, false, false)
            .unwrap()
            .trim(),
    )
    .unwrap();
    assert_eq!(json["context_metrics"]["route_classifications_total"], 3);
    assert_eq!(json["context_metrics"]["anchored_edit_rejected_total"], 1);
    assert_eq!(
        json["context_metrics"]["estimated_llm_calls_avoided_total"],
        2
    );
}
