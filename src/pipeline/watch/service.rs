use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    time::Duration,
};

use notify_debouncer_full::{new_debouncer, notify::RecursiveMode};

use crate::config::Config;

pub(super) use super::filter::filter_repo_events;
use super::{
    control::WatchControlResponse,
    control_bridge::spawn_control_listener,
    embeddings::{
        run_manual_embedding_build, EmbeddingJobContext, EmbeddingRefreshScheduler,
        ReconcileEmbeddingObservation,
    },
    events::{SyncTrigger, WatchEvent},
    filter::{collect_repo_paths, ignored_generated_dirs},
    lease::{acquire_watch_daemon_lease, WatchServiceMode},
    loop_message::LoopMessage,
    pending::PendingWatchChanges,
    reconcile::{run_reconcile_attempt, run_reconcile_attempt_with_touched_paths},
    reconcile_state::persist_reconcile_attempt_state,
    suppression::SuppressedPaths,
    sync::{
        emit_event, maybe_run_post_reconcile_auto_sync, run_sync_under_watch_lock, WatchSyncContext,
    },
};

/// Configuration for the watch and reconcile loop.
#[derive(Clone, Debug)]
pub struct WatchConfig {
    /// How long after the last filesystem event to wait before triggering a
    /// reconcile pass.
    pub debounce_timeout: Duration,
    /// Upper bound on events logged per reconcile cycle.
    pub max_events_per_cycle: usize,
}

impl Default for WatchConfig {
    fn default() -> Self {
        Self {
            debounce_timeout: Duration::from_millis(500),
            max_events_per_cycle: 1000,
        }
    }
}

/// Run the watch service in the current process.
pub fn run_watch_service(
    repo_root: &Path,
    config: &Config,
    watch_config: &WatchConfig,
    synrepo_dir: &Path,
    mode: WatchServiceMode,
    events: Option<crossbeam_channel::Sender<WatchEvent>>,
) -> crate::Result<()> {
    let (_lease, state_handle) = acquire_watch_daemon_lease(synrepo_dir, mode)
        .map_err(|error| crate::Error::Other(anyhow::anyhow!(error.to_string())))?;

    let stop_flag = Arc::new(AtomicBool::new(false));
    let auto_sync_enabled = Arc::new(AtomicBool::new(config.auto_sync_enabled));
    let auto_sync_blocked = Arc::new(AtomicBool::new(false));
    let mut embedding_refresh = EmbeddingRefreshScheduler::default();
    state_handle.note_auto_sync_enabled(config.auto_sync_enabled);
    let (tx, rx) = mpsc::channel::<LoopMessage>();
    let control_endpoint = state_handle.snapshot().control_endpoint;
    let socket_thread = spawn_control_listener(
        control_endpoint,
        state_handle.clone(),
        tx.clone(),
        stop_flag.clone(),
        auto_sync_enabled.clone(),
        auto_sync_blocked.clone(),
        config.watch_sync_timeout_seconds,
    )?;

    emit_event(&events, |now| WatchEvent::ReconcileStarted {
        at: now,
        triggering_events: 0,
        full: true,
        reason: None,
    });
    let startup_attempt = run_reconcile_attempt(repo_root, config, synrepo_dir, false);
    let startup = startup_attempt.outcome.clone();
    persist_reconcile_attempt_state(synrepo_dir, &startup_attempt, 0);
    state_handle.note_reconcile(&startup, 0);
    tracing::info!(outcome = %startup.as_str(), "startup reconcile complete");
    emit_event(&events, |now| WatchEvent::ReconcileFinished {
        at: now,
        outcome: startup.clone(),
        triggering_events: 0,
    });

    let pending_watch = Arc::new(Mutex::new(PendingWatchChanges::default()));
    let suppressed_paths = Arc::new(Mutex::new(SuppressedPaths::default()));
    let watch_roots = crate::substrate::discover_roots(repo_root, config);
    let watch_root_paths: Vec<_> = watch_roots
        .iter()
        .map(|root| root.absolute_path.clone())
        .collect();
    let callback_repo_root = repo_root.to_path_buf();
    let callback_repo_roots = watch_root_paths.clone();
    let callback_synrepo_dir = synrepo_dir.to_path_buf();
    let callback_ignored_dirs = ignored_generated_dirs(&callback_repo_roots, config);
    let callback_state_handle = state_handle.clone();
    let callback_events = events.clone();
    let pending_watch_for_callback = pending_watch.clone();
    let suppressed_paths_for_callback = suppressed_paths.clone();
    let max_events_per_cycle = watch_config.max_events_per_cycle;
    let mut debouncer =
        new_debouncer(
            watch_config.debounce_timeout,
            None,
            move |result| match result {
                Ok(watcher_events) => {
                    let filtered = filter_repo_events(
                        watcher_events,
                        &callback_repo_roots,
                        &callback_repo_root,
                        &callback_synrepo_dir,
                        &callback_ignored_dirs,
                    );
                    if filtered.is_empty() {
                        return;
                    }
                    let mut touched_paths = collect_repo_paths(
                        &filtered,
                        &callback_repo_roots,
                        &callback_repo_root,
                        &callback_synrepo_dir,
                        &callback_ignored_dirs,
                    );
                    if let Ok(mut suppressed) = suppressed_paths_for_callback.lock() {
                        suppressed.retain_unsuppressed(&mut touched_paths);
                    }
                    if touched_paths.is_empty() {
                        return;
                    }
                    callback_state_handle.note_event();
                    if let Ok(mut pending) = pending_watch_for_callback.lock() {
                        pending.record(filtered.len(), touched_paths, max_events_per_cycle);
                    }
                }
                Err(errors) => {
                    for error in &errors {
                        tracing::warn!("watcher error: {error}");
                        emit_event(&callback_events, |now| WatchEvent::Error {
                            at: now,
                            message: format!("watcher error: {error}"),
                        });
                    }
                }
            },
        )
        .map_err(|error| {
            crate::Error::Other(anyhow::anyhow!("failed to create file watcher: {error}"))
        })?;

    for root in &watch_root_paths {
        debouncer
            .watch(root, RecursiveMode::Recursive)
            .map_err(|error| {
                crate::Error::Other(anyhow::anyhow!(
                    "failed to watch {}: {error}",
                    root.display()
                ))
            })?;
    }

    let sync_context = WatchSyncContext {
        repo_root,
        config,
        synrepo_dir,
        events: &events,
        state_handle: &state_handle,
    };

    maybe_run_post_reconcile_auto_sync(
        &sync_context,
        &startup,
        &auto_sync_enabled,
        &auto_sync_blocked,
    );

    let mut last_reconcile_at = std::time::Instant::now();
    let keepalive_interval = config.reconcile_keepalive_seconds;

    loop {
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(message) if stop_flag.load(Ordering::Relaxed) => match message {
                LoopMessage::Stop => break,
                _ => break,
            },
            Err(mpsc::RecvTimeoutError::Timeout) => {
                embedding_refresh.reap_finished(&state_handle);
                if embedding_refresh.is_running() {
                    continue;
                }
                let pending_empty = pending_watch
                    .lock()
                    .map(|pending| pending.is_empty())
                    .unwrap_or(false);
                if pending_empty
                    && embedding_refresh.maybe_start_auto_refresh(
                        EmbeddingJobContext::new(
                            config,
                            synrepo_dir,
                            events.clone(),
                            state_handle.clone(),
                            stop_flag.clone(),
                        ),
                        &auto_sync_enabled,
                        &auto_sync_blocked,
                        false,
                    )
                {
                    continue;
                }
                let mut keepalive = false;
                let batch = match pending_watch.lock() {
                    Ok(mut pending) => {
                        if pending.is_empty() {
                            let due = keepalive_interval > 0
                                && last_reconcile_at.elapsed().as_secs()
                                    >= keepalive_interval as u64;
                            if !due {
                                continue;
                            }
                            keepalive = true;
                        }
                        let batch = pending.take(watch_config.max_events_per_cycle);
                        pending.clear_paths();
                        batch
                    }
                    Err(_) => continue,
                };

                let event_count = batch.event_count;
                tracing::debug!(events = event_count, keepalive, "running reconcile pass");
                let force_full_reconcile = batch.force_full_reconcile;
                let reason = force_full_reconcile
                    .then_some(super::events::ReconcileStartReason::WatchPathOverflow);
                emit_event(&events, |now| WatchEvent::ReconcileStarted {
                    at: now,
                    triggering_events: event_count,
                    full: force_full_reconcile || batch.touched_paths.is_empty(),
                    reason,
                });
                // Keepalive is fast: refresh the timestamp and auto-sync hook.
                let touched_paths = if force_full_reconcile || batch.touched_paths.is_empty() {
                    None
                } else {
                    Some(batch.touched_paths.as_slice())
                };
                let attempt = run_reconcile_attempt_with_touched_paths(
                    repo_root,
                    config,
                    synrepo_dir,
                    touched_paths,
                    keepalive,
                );
                let outcome = attempt.outcome.clone();
                persist_reconcile_attempt_state(synrepo_dir, &attempt, event_count);
                state_handle.note_reconcile(&outcome, event_count);
                last_reconcile_at = std::time::Instant::now();
                tracing::info!(
                    outcome = %outcome.as_str(),
                    events = event_count,
                    "reconcile pass complete"
                );
                emit_event(&events, |now| WatchEvent::ReconcileFinished {
                    at: now,
                    outcome: outcome.clone(),
                    triggering_events: event_count,
                });
                embedding_refresh.note_reconcile(
                    config,
                    synrepo_dir,
                    ReconcileEmbeddingObservation {
                        outcome: &outcome,
                        triggering_events: event_count,
                        force_full_reconcile,
                        keepalive,
                    },
                    &state_handle,
                );
                maybe_run_post_reconcile_auto_sync(
                    &sync_context,
                    &outcome,
                    &auto_sync_enabled,
                    &auto_sync_blocked,
                );
            }
            Ok(LoopMessage::Stop) => {
                break;
            }
            Ok(LoopMessage::SuppressPaths {
                respond_to,
                paths,
                ttl,
            }) => {
                if let Ok(mut suppressed) = suppressed_paths.lock() {
                    suppressed.suppress(paths, ttl);
                }
                let _ = respond_to.send(WatchControlResponse::Ack {
                    message: "paths suppressed".to_string(),
                });
            }
            Ok(LoopMessage::ReconcileNow { respond_to, fast }) => {
                emit_event(&events, |now| WatchEvent::ReconcileStarted {
                    at: now,
                    triggering_events: 0,
                    full: true,
                    reason: None,
                });
                let attempt = run_reconcile_attempt(repo_root, config, synrepo_dir, fast);
                let outcome = attempt.outcome.clone();
                persist_reconcile_attempt_state(synrepo_dir, &attempt, 0);
                state_handle.note_reconcile(&outcome, 0);
                last_reconcile_at = std::time::Instant::now();
                emit_event(&events, |now| WatchEvent::ReconcileFinished {
                    at: now,
                    outcome: outcome.clone(),
                    triggering_events: 0,
                });
                let outcome_for_response = outcome.clone();
                let _ = respond_to.send(WatchControlResponse::Reconcile {
                    outcome: outcome_for_response,
                    triggering_events: 0,
                });
                maybe_run_post_reconcile_auto_sync(
                    &sync_context,
                    &outcome,
                    &auto_sync_enabled,
                    &auto_sync_blocked,
                );
            }
            Ok(LoopMessage::SyncNow {
                respond_to,
                options,
            }) => {
                let response =
                    run_sync_under_watch_lock(&sync_context, options, None, SyncTrigger::Manual);
                let _ = respond_to.send(response);
            }
            Ok(LoopMessage::EmbeddingsBuildNow { respond_to }) => {
                let response = if embedding_refresh.is_running() {
                    WatchControlResponse::Error {
                        message: "embedding refresh already running".to_string(),
                    }
                } else {
                    let response = run_manual_embedding_build(EmbeddingJobContext::new(
                        config,
                        synrepo_dir,
                        events.clone(),
                        state_handle.clone(),
                        stop_flag.clone(),
                    ));
                    if matches!(response, WatchControlResponse::EmbeddingsBuild { .. }) {
                        embedding_refresh.clear_stale(&state_handle);
                    }
                    response
                };
                let _ = respond_to.send(response);
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    stop_flag.store(true, Ordering::Relaxed);
    drop(debouncer);
    embedding_refresh.join_on_stop();
    let _ = socket_thread.join();
    Ok(())
}

/// Run the watch loop in the foreground.
pub fn run_watch_loop(
    repo_root: &Path,
    config: &Config,
    watch_config: &WatchConfig,
    synrepo_dir: &Path,
) -> crate::Result<()> {
    run_watch_service(
        repo_root,
        config,
        watch_config,
        synrepo_dir,
        WatchServiceMode::Foreground,
        None,
    )
}
