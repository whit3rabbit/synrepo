//! Sync helpers used by the watch service.

use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

use crate::{
    config::Config,
    pipeline::{
        repair::{
            execute_sync_locked_with_stop, RepairSurface, SyncOptions, SyncProgress, SyncSummary,
        },
        writer::{acquire_writer_lock, LockError, WriterLock},
    },
};

use super::{
    control::WatchControlResponse,
    events::{SyncTrigger, WatchEvent},
    lease::WatchStateHandle,
};

/// Shared inputs for a sync pass run by the watch service.
pub(super) struct WatchSyncContext<'a> {
    pub(super) repo_root: &'a Path,
    pub(super) config: &'a Config,
    pub(super) synrepo_dir: &'a Path,
    pub(super) events: &'a Option<crossbeam_channel::Sender<WatchEvent>>,
    pub(super) state_handle: &'a WatchStateHandle,
    pub(super) stop_flag: Option<&'a AtomicBool>,
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

/// Acquire the raw writer lock and run one sync pass on the watch mutation
/// worker. Emits `SyncStarted`/`SyncProgress`/`SyncFinished` events and returns
/// the appropriate `WatchControlResponse`.
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
    let mut stop_requested = || {
        context
            .stop_flag
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
    };
    let mut should_stop: Option<&mut dyn FnMut() -> bool> =
        context.stop_flag.map(|_| &mut stop_requested as _);

    let summary = match execute_sync_locked_with_stop(
        context.repo_root,
        context.synrepo_dir,
        context.config,
        options,
        &mut progress,
        surface_filter,
        &mut should_stop,
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
