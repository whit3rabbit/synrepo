//! Main-loop coordination for completed and newly scheduled watch operations.

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    time::Instant,
};

use crate::config::Config;

use super::{
    control::WatchControlResponse,
    embeddings::{EmbeddingRefreshScheduler, ReconcileEmbeddingObservation},
    events::{SyncTrigger, WatchEvent},
    lease::WatchStateHandle,
    loop_message::LoopMessage,
    pending::PendingWatchChanges,
    reconcile::ReconcileOutcome,
    sync::emit_event,
    worker::{
        ReconcileWork, WatchOperation, WatchOperationCompletion, WatchOperationContext,
        WatchOperationKind, WatchOperationResult, WatchOperationScheduler,
    },
};

#[allow(clippy::too_many_arguments)]
pub(super) fn handle_completion(
    completion: WatchOperationCompletion,
    config: &Config,
    synrepo_dir: &std::path::Path,
    state_handle: &WatchStateHandle,
    embedding_refresh: &mut EmbeddingRefreshScheduler,
    pending_watch: &Arc<Mutex<PendingWatchChanges>>,
    auto_sync_enabled: &AtomicBool,
    auto_sync_blocked: &AtomicBool,
    pending_auto_sync: &mut bool,
    last_reconcile_at: &mut Instant,
) {
    let respond_to = completion.respond_to;
    let result = match completion.result {
        Ok(result) => result,
        Err(message) => {
            if completion.kind == WatchOperationKind::Reconcile {
                requeue_full_reconcile(pending_watch);
            }
            if completion.kind == WatchOperationKind::Embeddings {
                state_handle.note_embedding_error(message.clone());
            }
            if completion.kind == WatchOperationKind::Sync {
                state_handle.note_auto_sync_error(message.clone());
            }
            send_response(respond_to, WatchControlResponse::Error { message });
            return;
        }
    };

    match result {
        WatchOperationResult::Reconcile { work, outcome } => {
            finish_reconcile(
                work,
                &outcome,
                config,
                synrepo_dir,
                state_handle,
                embedding_refresh,
                pending_watch,
                auto_sync_enabled,
                auto_sync_blocked,
                pending_auto_sync,
                last_reconcile_at,
            );
            send_response(
                respond_to,
                WatchControlResponse::Reconcile {
                    outcome,
                    triggering_events: 0,
                },
            );
        }
        WatchOperationResult::Sync { trigger, response } => {
            if let WatchControlResponse::Sync { summary } = &response {
                auto_sync_blocked.store(!summary.blocked.is_empty(), Ordering::Relaxed);
                if trigger == SyncTrigger::Manual && summary.blocked.is_empty() {
                    *pending_auto_sync = false;
                }
            }
            send_response(respond_to, response);
        }
        WatchOperationResult::ManualEmbeddings { response } => {
            if matches!(response, WatchControlResponse::EmbeddingsBuild { .. }) {
                embedding_refresh.clear_stale(state_handle);
            }
            send_response(respond_to, response);
        }
        WatchOperationResult::AutoEmbeddings { result } => {
            embedding_refresh.note_auto_refresh_finished(&result, state_handle);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn finish_reconcile(
    work: ReconcileWork,
    outcome: &ReconcileOutcome,
    config: &Config,
    synrepo_dir: &std::path::Path,
    state_handle: &WatchStateHandle,
    embedding_refresh: &mut EmbeddingRefreshScheduler,
    pending_watch: &Arc<Mutex<PendingWatchChanges>>,
    auto_sync_enabled: &AtomicBool,
    auto_sync_blocked: &AtomicBool,
    pending_auto_sync: &mut bool,
    last_reconcile_at: &mut Instant,
) {
    *last_reconcile_at = Instant::now();
    let (event_count, keepalive, failed_batch, observe_embeddings) = match work {
        ReconcileWork::Startup => (0, false, None, false),
        ReconcileWork::Manual { .. } => (0, false, None, true),
        ReconcileWork::Watch { batch, keepalive } => {
            let failed_batch = (!matches!(outcome, ReconcileOutcome::Completed(_)))
                .then_some((batch.touched_paths.clone(), batch.force_full_reconcile));
            (batch.event_count, keepalive, failed_batch, true)
        }
    };

    if let Some((paths, force_full)) = failed_batch {
        if let Ok(mut pending) = pending_watch.lock() {
            pending.requeue_failed(paths, force_full);
        }
    }
    if observe_embeddings {
        embedding_refresh.note_reconcile(
            config,
            synrepo_dir,
            ReconcileEmbeddingObservation {
                outcome,
                triggering_events: event_count,
                keepalive,
            },
            state_handle,
        );
    }
    if matches!(outcome, ReconcileOutcome::Completed(_))
        && auto_sync_enabled.load(Ordering::Relaxed)
        && !auto_sync_blocked.load(Ordering::Relaxed)
    {
        *pending_auto_sync = true;
    }
}

pub(super) fn start_manual_operation(
    scheduler: &mut WatchOperationScheduler,
    context: WatchOperationContext,
    operation: WatchOperation,
    respond_to: mpsc::Sender<WatchControlResponse>,
) -> bool {
    match scheduler.start(context, operation, Some(respond_to.clone())) {
        Ok(()) => true,
        Err(message) => {
            let _ = respond_to.send(WatchControlResponse::Error { message });
            false
        }
    }
}

pub(super) fn start_automatic_operation(
    scheduler: &mut WatchOperationScheduler,
    context: WatchOperationContext,
    operation: WatchOperation,
    events: &Option<crossbeam_channel::Sender<WatchEvent>>,
) -> bool {
    if let Err(message) = scheduler.start(context, operation, None) {
        emit_event(events, |now| WatchEvent::Error {
            at: now,
            message: message.clone(),
        });
        return false;
    }
    true
}

pub(super) fn reject_message_while_stopping(message: LoopMessage) {
    let respond_to = match message {
        LoopMessage::Stop => return,
        LoopMessage::ReconcileNow { respond_to, .. }
        | LoopMessage::SyncNow { respond_to, .. }
        | LoopMessage::EmbeddingsBuildNow { respond_to }
        | LoopMessage::SuppressPaths { respond_to, .. } => respond_to,
    };
    let _ = respond_to.send(WatchControlResponse::Error {
        message: "watch service is stopping".to_string(),
    });
}

fn requeue_full_reconcile(pending_watch: &Arc<Mutex<PendingWatchChanges>>) {
    if let Ok(mut pending) = pending_watch.lock() {
        pending.record_full(1);
    }
}

fn send_response(
    respond_to: Option<mpsc::Sender<WatchControlResponse>>,
    response: WatchControlResponse,
) {
    if let Some(respond_to) = respond_to {
        let _ = respond_to.send(response);
    }
}
