use crate::pipeline::watch::{
    run_reconcile_pass, ReconcileOutcome, WatchControlRequest, WatchControlResponse,
    WatchServiceStatus,
};
use crate::pipeline::writer::acquire_write_admission;

use super::helpers::{load_repo_config, lock_error_to_action};
use super::{ActionContext, ActionOutcome};

/// Run a reconcile pass on behalf of the dashboard. If a watch service owns
/// the repo, delegate via the control socket; otherwise acquire the writer
/// lock and run one pass directly.
///
/// Never bypasses the writer lock. On contention the caller receives an
/// [`ActionOutcome::Conflict`] they can surface in the log pane.
pub fn reconcile_now(ctx: &ActionContext) -> ActionOutcome {
    match crate::pipeline::watch::watch_service_status(&ctx.synrepo_dir) {
        WatchServiceStatus::Running(state) => {
            match crate::pipeline::watch::request_watch_control(
                &ctx.synrepo_dir,
                WatchControlRequest::ReconcileNow,
            ) {
                Ok(WatchControlResponse::Reconcile { outcome, .. }) => {
                    reconcile_outcome_to_action(&outcome)
                }
                Ok(WatchControlResponse::Error { message }) => ActionOutcome::Error { message },
                Ok(_) => ActionOutcome::Error {
                    message: format!(
                        "watch service (pid {}) returned an unexpected response to reconcile-now",
                        state.pid
                    ),
                },
                Err(err) => ActionOutcome::Error {
                    message: format!("reconcile-now delegate failed: {err}"),
                },
            }
        }
        WatchServiceStatus::Starting => ActionOutcome::Conflict {
            owner_pid: None,
            acquired_at: None,
            surface: "watch lease".to_string(),
            guidance:
                "watch service is still starting; wait for it to become ready before reconciling"
                    .to_string(),
        },
        WatchServiceStatus::Inactive => run_local_reconcile(ctx),
        WatchServiceStatus::Stale(_) | WatchServiceStatus::Corrupt(_) => {
            if let Err(err) =
                crate::pipeline::watch::cleanup_stale_watch_artifacts(&ctx.synrepo_dir)
            {
                return ActionOutcome::Error {
                    message: format!("failed to clean stale watch artifacts: {err}"),
                };
            }
            run_local_reconcile(ctx)
        }
    }
}

pub(super) fn run_local_reconcile(ctx: &ActionContext) -> ActionOutcome {
    let config = match load_repo_config(ctx, "reconcile") {
        Ok(c) => c,
        Err(outcome) => return outcome,
    };

    match acquire_write_admission(&ctx.synrepo_dir, "reconcile") {
        Ok(_lock) => {
            let outcome = run_reconcile_pass(&ctx.repo_root, &config, &ctx.synrepo_dir);
            reconcile_outcome_to_action(&outcome)
        }
        Err(err) => lock_error_to_action(&ctx.synrepo_dir, err),
    }
}

/// Convert a `ReconcileOutcome` from an in-process or delegated pass into a
/// structured action outcome.
pub(super) fn reconcile_outcome_to_action(outcome: &ReconcileOutcome) -> ActionOutcome {
    match outcome {
        ReconcileOutcome::Completed(summary) => ActionOutcome::Completed {
            message: format!(
                "reconcile completed ({} files, {} symbols)",
                summary.files_discovered, summary.symbols_extracted
            ),
        },
        ReconcileOutcome::LockConflict { holder_pid } => ActionOutcome::Conflict {
            owner_pid: Some(*holder_pid),
            acquired_at: None,
            surface: "writer lock".to_string(),
            guidance: format!("reconcile skipped: writer lock held by pid {holder_pid}"),
        },
        ReconcileOutcome::Failed(message) => ActionOutcome::Error {
            message: format!("reconcile failed: {message}"),
        },
    }
}
