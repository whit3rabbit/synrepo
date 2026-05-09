use serde_json::json;

use crate::{
    pipeline::watch::{
        cleanup_stale_watch_artifacts, persist_reconcile_attempt_state, request_watch_control,
        run_reconcile_attempt, watch_service_status, ReconcileOutcome, WatchControlRequest,
        WatchControlResponse, WatchServiceStatus,
    },
    surface::mcp::SynrepoState,
};

pub fn post_edit_diagnostics(
    state: &SynrepoState,
    synrepo_dir: &std::path::Path,
    touched: &[String],
    diagnostics_budget: Option<&str>,
) -> serde_json::Value {
    let reconcile = reconcile_after_edit(state, synrepo_dir);
    json!({
        "validation": "passed",
        "write_status": "applied",
        "reconcile": reconcile,
        "diagnostics_budget": diagnostics_budget.unwrap_or("default"),
        "test_surface_recommendations": touched.iter().map(|path| {
            json!({
                "path": path,
                "tool": "synrepo_tests",
                "args": { "scope": path, "budget": "tiny" },
                "status": "recommended"
            })
        }).collect::<Vec<_>>(),
        "command_execution": "unavailable",
    })
}

fn reconcile_after_edit(state: &SynrepoState, synrepo_dir: &std::path::Path) -> serde_json::Value {
    match watch_service_status(synrepo_dir) {
        WatchServiceStatus::Running(watch) => {
            match request_watch_control(
                synrepo_dir,
                WatchControlRequest::ReconcileNow { fast: false },
            ) {
                Ok(WatchControlResponse::Reconcile {
                    outcome,
                    triggering_events,
                }) => json!({
                    "status": "delegated",
                    "watch_pid": watch.pid,
                    "triggering_events": triggering_events,
                    "outcome": reconcile_outcome_json(&outcome),
                }),
                Ok(WatchControlResponse::Error { message }) => json!({
                    "status": "failed",
                    "watch_pid": watch.pid,
                    "message": message,
                    "operator_action": "run `synrepo watch stop`, then `synrepo reconcile`",
                }),
                Ok(other) => json!({
                    "status": "failed",
                    "watch_pid": watch.pid,
                    "message": format!("unexpected watch response: {other:?}"),
                }),
                Err(err) => json!({
                    "status": "failed",
                    "watch_pid": watch.pid,
                    "message": err.to_string(),
                    "operator_action": "run `synrepo watch stop`, then `synrepo reconcile`",
                }),
            }
        }
        WatchServiceStatus::Starting => json!({
            "status": "unknown",
            "message": "watch service is starting",
            "operator_action": "retry after watch startup, or run `synrepo reconcile`",
        }),
        WatchServiceStatus::Stale(_) | WatchServiceStatus::Corrupt(_) => {
            let _ = cleanup_stale_watch_artifacts(synrepo_dir);
            run_local_reconcile(state, synrepo_dir)
        }
        WatchServiceStatus::Inactive => run_local_reconcile(state, synrepo_dir),
    }
}

fn run_local_reconcile(state: &SynrepoState, synrepo_dir: &std::path::Path) -> serde_json::Value {
    let attempt = run_reconcile_attempt(&state.repo_root, &state.config, synrepo_dir, false);
    let outcome = attempt.outcome.clone();
    persist_reconcile_attempt_state(synrepo_dir, &attempt, 0);
    json!({
        "status": "local",
        "outcome": reconcile_outcome_json(&outcome),
    })
}

fn reconcile_outcome_json(outcome: &ReconcileOutcome) -> serde_json::Value {
    match outcome {
        ReconcileOutcome::Completed(summary) => json!({
            "kind": "completed",
            "files_discovered": summary.files_discovered,
            "symbols_extracted": summary.symbols_extracted,
        }),
        ReconcileOutcome::LockConflict { holder_pid } => json!({
            "kind": "lock_conflict",
            "holder_pid": holder_pid,
        }),
        ReconcileOutcome::Failed(message) => json!({
            "kind": "failed",
            "message": message,
        }),
    }
}
