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
    reconcile::ReconcileOutcome,
    service::{SyncTrigger, WatchEvent},
};

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
    repo_root: &Path,
    config: &Config,
    synrepo_dir: &Path,
    options: SyncOptions,
    surface_filter: Option<&'static [RepairSurface]>,
    trigger: SyncTrigger,
    events: &Option<crossbeam_channel::Sender<WatchEvent>>,
) -> WatchControlResponse {
    emit_event(events, |now| WatchEvent::SyncStarted { at: now, trigger });

    let _lock: WriterLock = match acquire_writer_lock(synrepo_dir) {
        Ok(lock) => lock,
        Err(LockError::HeldByOther { pid, .. }) => {
            let msg =
                format!("sync: writer lock held by pid {pid}; watch main loop could not acquire");
            emit_event(events, |now| WatchEvent::SyncFinished {
                at: now,
                trigger,
                summary: empty_sync_summary(),
            });
            return WatchControlResponse::Error { message: msg };
        }
        Err(err) => {
            emit_event(events, |now| WatchEvent::SyncFinished {
                at: now,
                trigger,
                summary: empty_sync_summary(),
            });
            return WatchControlResponse::Error {
                message: format!("sync: could not acquire writer lock: {err}"),
            };
        }
    };

    let events_for_cb = events.clone();
    let mut progress_cb = move |progress: SyncProgress| {
        emit_event(&events_for_cb, |now| WatchEvent::SyncProgress {
            at: now,
            progress: progress.clone(),
        });
    };

    let mut progress: Option<&mut dyn FnMut(SyncProgress)> = Some(&mut progress_cb);

    let summary = match execute_sync_locked(
        repo_root,
        synrepo_dir,
        config,
        options,
        &mut progress,
        surface_filter,
    ) {
        Ok(summary) => summary,
        Err(err) => {
            emit_event(events, |now| WatchEvent::SyncFinished {
                at: now,
                trigger,
                summary: empty_sync_summary(),
            });
            return WatchControlResponse::Error {
                message: format!("sync failed: {err}"),
            };
        }
    };

    emit_event(events, |now| WatchEvent::SyncFinished {
        at: now,
        trigger,
        summary: summary.clone(),
    });

    WatchControlResponse::Sync { summary }
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
    repo_root: &Path,
    config: &Config,
    synrepo_dir: &Path,
    outcome: &ReconcileOutcome,
    auto_sync_enabled: &AtomicBool,
    auto_sync_blocked: &AtomicBool,
    events: &Option<crossbeam_channel::Sender<WatchEvent>>,
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
        repo_root,
        config,
        synrepo_dir,
        SyncOptions::default(),
        Some(CHEAP_AUTO_SYNC_SURFACES),
        SyncTrigger::AutoPostReconcile,
        events,
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
