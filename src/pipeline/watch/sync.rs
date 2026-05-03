//! Sync helpers used by the watch service.

use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

use crate::{
    config::Config,
    pipeline::{
        repair::{
            execute_sync_locked, RepairSurface, SyncOptions, SyncProgress, SyncSummary,
            CHEAP_AUTO_SYNC_SURFACES,
        },
        writer::{acquire_writer_lock, LockError, WriterLock},
    },
};

use super::{
    control::WatchControlResponse,
    events::{SyncTrigger, WatchEvent},
    lease::WatchStateHandle,
    reconcile::ReconcileOutcome,
};

/// Shared inputs for a sync pass run by the watch service.
pub(super) struct WatchSyncContext<'a> {
    pub(super) repo_root: &'a Path,
    pub(super) config: &'a Config,
    pub(super) synrepo_dir: &'a Path,
    pub(super) events: &'a Option<crossbeam_channel::Sender<WatchEvent>>,
    pub(super) state_handle: &'a WatchStateHandle,
}

/// Best-effort send on the optional event channel. A dropped receiver must
/// not kill the watch loop, so failures are swallowed.
pub(super) fn emit_event<F>(sender: &Option<crossbeam_channel::Sender<WatchEvent>>, build: F)
where
    F: FnOnce(String) -> WatchEvent,
{
    if let Some(tx) = sender {
        let event = build(crate::pipeline::writer::now_rfc3339());
        let _ = tx.try_send(event);
    }
}

/// Acquire the raw writer lock and run one sync pass inline. Runs on the
/// watch main-loop thread. Emits `SyncStarted`/`SyncProgress`/`SyncFinished`
/// events and returns the appropriate `WatchControlResponse`.
pub(super) fn run_sync_under_watch_lock(
    context: &WatchSyncContext<'_>,
    options: SyncOptions,
    surface_filter: Option<&'static [RepairSurface]>,
    trigger: SyncTrigger,
) -> WatchControlResponse {
    if matches!(trigger, SyncTrigger::AutoPostReconcile) {
        context.state_handle.note_auto_sync_started();
    }
    emit_event(context.events, |now| WatchEvent::SyncStarted {
        at: now,
        trigger,
    });

    let _lock: WriterLock = match acquire_writer_lock(context.synrepo_dir) {
        Ok(lock) => lock,
        Err(LockError::HeldByOther { pid, .. }) => {
            let msg =
                format!("sync: writer lock held by pid {pid}; watch main loop could not acquire");
            note_sync_error(trigger, context.state_handle, &msg);
            emit_event(context.events, |now| WatchEvent::SyncFinished {
                at: now,
                trigger,
                summary: empty_sync_summary(),
            });
            return WatchControlResponse::Error { message: msg };
        }
        Err(err) => {
            let msg = format!("sync: could not acquire writer lock: {err}");
            note_sync_error(trigger, context.state_handle, &msg);
            emit_event(context.events, |now| WatchEvent::SyncFinished {
                at: now,
                trigger,
                summary: empty_sync_summary(),
            });
            return WatchControlResponse::Error { message: msg };
        }
    };

    let events_for_cb = context.events.clone();
    let mut progress_cb = move |progress: SyncProgress| {
        emit_event(&events_for_cb, |now| WatchEvent::SyncProgress {
            at: now,
            progress: progress.clone(),
        });
    };

    let mut progress: Option<&mut dyn FnMut(SyncProgress)> = Some(&mut progress_cb);

    let summary = match execute_sync_locked(
        context.repo_root,
        context.synrepo_dir,
        context.config,
        options,
        &mut progress,
        surface_filter,
    ) {
        Ok(summary) => summary,
        Err(err) => {
            let msg = format!("sync failed: {err}");
            note_sync_error(trigger, context.state_handle, &msg);
            emit_event(context.events, |now| WatchEvent::SyncFinished {
                at: now,
                trigger,
                summary: empty_sync_summary(),
            });
            return WatchControlResponse::Error { message: msg };
        }
    };

    note_sync_finished(trigger, context.state_handle, !summary.blocked.is_empty());
    emit_event(context.events, |now| WatchEvent::SyncFinished {
        at: now,
        trigger,
        summary: summary.clone(),
    });

    WatchControlResponse::Sync { summary }
}

fn note_sync_finished(trigger: SyncTrigger, state_handle: &WatchStateHandle, blocked: bool) {
    match trigger {
        SyncTrigger::AutoPostReconcile => state_handle.note_auto_sync_finished(blocked),
        SyncTrigger::Manual => state_handle.note_manual_sync_finished(blocked),
    }
}

fn note_sync_error(trigger: SyncTrigger, state_handle: &WatchStateHandle, message: &str) {
    if matches!(trigger, SyncTrigger::AutoPostReconcile) {
        state_handle.note_auto_sync_error(message);
    }
}

fn empty_sync_summary() -> SyncSummary {
    SyncSummary {
        synced_at: crate::pipeline::writer::now_rfc3339(),
        repaired: Vec::new(),
        report_only: Vec::new(),
        blocked: Vec::new(),
    }
}

/// Call the auto-sync hook if it is enabled and the reconcile pass produced a
/// non-failure outcome. Skips on prior-pass blocked findings to avoid tight
/// retry loops.
pub(super) fn maybe_run_post_reconcile_auto_sync(
    context: &WatchSyncContext<'_>,
    outcome: &ReconcileOutcome,
    auto_sync_enabled: &AtomicBool,
    auto_sync_blocked: &AtomicBool,
) {
    if !auto_sync_enabled.load(Ordering::Relaxed) {
        return;
    }
    if !matches!(outcome, ReconcileOutcome::Completed(_)) {
        return;
    }
    if auto_sync_blocked.load(Ordering::Relaxed) {
        // A previous auto-sync hit a blocked finding on a cheap surface. Skip
        // until the operator intervenes (runs `synrepo sync` manually or
        // toggles the flag off and on).
        return;
    }

    let response = run_sync_under_watch_lock(
        context,
        SyncOptions::default(),
        Some(CHEAP_AUTO_SYNC_SURFACES),
        SyncTrigger::AutoPostReconcile,
    );

    if let WatchControlResponse::Sync { summary } = response {
        if !summary.blocked.is_empty() {
            tracing::warn!(
                "auto-sync produced blocked findings on cheap surfaces; pausing auto-sync until reconcile succeeds cleanly"
            );
            auto_sync_blocked.store(true, Ordering::Relaxed);
        } else {
            // Successful auto-sync re-enables the loop if a previous block
            // cleared without operator intervention.
            auto_sync_blocked.store(false, Ordering::Relaxed);
        }
    }
}
