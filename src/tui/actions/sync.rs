use crate::pipeline::repair::{execute_sync, SyncOptions, SyncSummary};
use crate::pipeline::watch::{
    request_watch_control, WatchControlRequest, WatchControlResponse, WatchServiceStatus,
};

use super::helpers::{load_repo_config, lock_error_to_action};
use super::{ActionContext, ActionOutcome};

/// Run one repair sync pass on behalf of the dashboard.
///
/// When a watch service owns the repo, delegate via the control socket; the
/// watch process acquires its own writer lock and emits `SyncStarted` /
/// `SyncProgress` / `SyncFinished` events that the dashboard's log pane
/// already surfaces. Without watch, run `execute_sync` inline under the
/// standard writer-lock admission path.
pub fn sync_now(ctx: &ActionContext) -> ActionOutcome {
    match crate::pipeline::watch::watch_service_status(&ctx.synrepo_dir) {
        WatchServiceStatus::Running(state) => match request_watch_control(
            &ctx.synrepo_dir,
            WatchControlRequest::SyncNow {
                options: SyncOptions::default(),
            },
        ) {
            Ok(WatchControlResponse::Sync { summary }) => sync_summary_to_action(&summary),
            Ok(WatchControlResponse::Error { message }) => ActionOutcome::Error { message },
            Ok(_) => ActionOutcome::Error {
                message: format!(
                    "watch service (pid {}) returned an unexpected response to sync-now",
                    state.pid
                ),
            },
            Err(err) => ActionOutcome::Error {
                message: format!("sync-now delegate failed: {err}"),
            },
        },
        WatchServiceStatus::Starting => ActionOutcome::Conflict {
            owner_pid: None,
            acquired_at: None,
            surface: "watch lease".to_string(),
            guidance: "watch service is still starting; wait for it to become ready before syncing"
                .to_string(),
        },
        WatchServiceStatus::Inactive => run_local_sync(ctx),
        WatchServiceStatus::Stale(_) | WatchServiceStatus::Corrupt(_) => {
            if let Err(err) =
                crate::pipeline::watch::cleanup_stale_watch_artifacts(&ctx.synrepo_dir)
            {
                return ActionOutcome::Error {
                    message: format!("failed to clean stale watch artifacts: {err}"),
                };
            }
            run_local_sync(ctx)
        }
    }
}

fn run_local_sync(ctx: &ActionContext) -> ActionOutcome {
    let config = match load_repo_config(ctx, "sync") {
        Ok(c) => c,
        Err(outcome) => return outcome,
    };

    match execute_sync(
        &ctx.repo_root,
        &ctx.synrepo_dir,
        &config,
        SyncOptions::default(),
    ) {
        Ok(summary) => sync_summary_to_action(&summary),
        Err(err) => {
            let msg = err.to_string();
            // `execute_sync` maps writer-lock admission errors through
            // `map_lock_error` which yields opaque `anyhow` text. Detecting the
            // watch-active case here keeps the outcome structured so the log
            // pane reports it as a watch-lease conflict rather than a generic
            // error.
            if msg.contains("watch service is active") {
                return ActionOutcome::Conflict {
                    owner_pid: None,
                    acquired_at: None,
                    surface: "watch lease".to_string(),
                    guidance: "watch service is active for this repo; delegate via the socket"
                        .to_string(),
                };
            }
            if let Some(conflict) = extract_lock_conflict(&ctx.synrepo_dir, &msg) {
                return conflict;
            }
            ActionOutcome::Error {
                message: format!("sync failed: {msg}"),
            }
        }
    }
}

fn extract_lock_conflict(synrepo_dir: &std::path::Path, msg: &str) -> Option<ActionOutcome> {
    // Best-effort structural detection: if the error text names the writer
    // lock, re-issue the admission check directly so `lock_error_to_action`
    // can build a structured conflict outcome.
    if !msg.contains("writer lock") {
        return None;
    }
    let err = crate::pipeline::writer::acquire_write_admission(synrepo_dir, "sync").err()?;
    Some(lock_error_to_action(synrepo_dir, err))
}

fn sync_summary_to_action(summary: &SyncSummary) -> ActionOutcome {
    if summary.blocked.is_empty() {
        ActionOutcome::Completed {
            message: format!(
                "sync completed ({} repaired, {} report-only)",
                summary.repaired.len(),
                summary.report_only.len()
            ),
        }
    } else {
        ActionOutcome::Error {
            message: format!(
                "sync finished with {} blocked finding(s); run `synrepo check --json` for detail",
                summary.blocked.len()
            ),
        }
    }
}
