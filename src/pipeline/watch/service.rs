use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    time::Duration,
};

use crate::config::Config;
use notify_debouncer_full::notify::RecursiveMode;

use super::{
    branch_refs::BranchRefPoller,
    config::WatchConfig,
    control::WatchControlResponse,
    control_bridge::spawn_control_listener,
    coordinator::{
        handle_completion, reject_message_while_stopping, start_automatic_operation,
        start_manual_operation,
    },
    debouncer::new_watch_debouncer,
    embeddings::EmbeddingRefreshScheduler,
    events::{EmbeddingTrigger, SyncTrigger, WatchEvent},
    filter::{collect_repo_paths, filter_repo_events, ignored_generated_dirs, WatchIgnoreSet},
    lease::{acquire_watch_daemon_lease, WatchServiceMode},
    loop_message::LoopMessage,
    pending::PendingWatchChanges,
    suppression::SuppressedPaths,
    sync::emit_event,
    worker::{ReconcileWork, WatchOperation, WatchOperationContext, WatchOperationScheduler},
};

/// Run the watch service in the current process.
pub fn run_watch_service(
    repo_root: &Path,
    config: &Config,
    watch_config: &WatchConfig,
    synrepo_dir: &Path,
    mode: WatchServiceMode,
    events: Option<crossbeam_channel::Sender<WatchEvent>>,
) -> crate::Result<()> {
    run_watch_service_with_shutdown(
        repo_root,
        config,
        watch_config,
        synrepo_dir,
        mode,
        events,
        false,
    )
}

/// Run the detached-daemon variant of the service.
/// Unlike the embedded path, this may stop waiting for a stuck worker after a
/// bounded shutdown delay because the daemon process exits immediately after
/// this function returns.
#[doc(hidden)]
pub fn run_watch_service_process_owned(
    repo_root: &Path,
    config: &Config,
    watch_config: &WatchConfig,
    synrepo_dir: &Path,
    mode: WatchServiceMode,
    events: Option<crossbeam_channel::Sender<WatchEvent>>,
) -> crate::Result<()> {
    run_watch_service_with_shutdown(
        repo_root,
        config,
        watch_config,
        synrepo_dir,
        mode,
        events,
        true,
    )
}

fn run_watch_service_with_shutdown(
    repo_root: &Path,
    config: &Config,
    watch_config: &WatchConfig,
    synrepo_dir: &Path,
    mode: WatchServiceMode,
    events: Option<crossbeam_channel::Sender<WatchEvent>>,
    process_exits_after_return: bool,
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

    let pending_watch = Arc::new(Mutex::new(PendingWatchChanges::default()));
    let mut branch_ref_poller = BranchRefPoller::new(repo_root, config);
    let suppressed_paths = Arc::new(Mutex::new(SuppressedPaths::default()));
    let watch_roots = crate::substrate::discover_roots(repo_root, config);
    let watch_root_paths: Vec<_> = watch_roots
        .iter()
        .filter(|root| root.editable)
        .map(|root| root.absolute_path.clone())
        .collect();
    let callback_repo_root = repo_root.to_path_buf();
    let callback_repo_roots = watch_root_paths.clone();
    let callback_synrepo_dir = synrepo_dir.to_path_buf();
    let callback_ignored_dirs = ignored_generated_dirs(&callback_repo_roots, config);
    let callback_ignore_set = WatchIgnoreSet::from_roots(&callback_repo_roots);
    let callback_state_handle = state_handle.clone();
    let callback_events = events.clone();
    let pending_watch_for_callback = pending_watch.clone();
    let suppressed_paths_for_callback = suppressed_paths.clone();
    let max_events_per_cycle = watch_config.max_events_per_cycle;
    let mut debouncer =
        new_watch_debouncer(
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
                        &callback_ignore_set,
                    );
                    if filtered.is_empty() {
                        return;
                    }
                    let mut collected = collect_repo_paths(
                        &filtered,
                        &callback_repo_roots,
                        &callback_repo_root,
                        &callback_synrepo_dir,
                        &callback_ignored_dirs,
                        &callback_ignore_set,
                    );
                    if let Ok(mut suppressed) = suppressed_paths_for_callback.lock() {
                        suppressed.filter_collected(&mut collected);
                    }
                    if collected.is_empty() {
                        return;
                    }
                    callback_state_handle.note_event();
                    if let Ok(mut pending) = pending_watch_for_callback.lock() {
                        if collected.has_directory_event() {
                            pending.record_full(filtered.len());
                        } else {
                            pending.record(filtered.len(), collected.paths, max_events_per_cycle);
                        }
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

    // Register watches BEFORE running the startup reconcile so that any edit
    // arriving during the initial scan is captured in `pending_watch` and
    // processed on the first loop tick. (Apple FSEvents recommendation: begin
    // monitoring before scanning so directories modified during the scan are
    // revisited.)
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

    let operation_context = WatchOperationContext::new(
        repo_root.to_path_buf(),
        config.clone(),
        synrepo_dir.to_path_buf(),
        events.clone(),
        state_handle.clone(),
        stop_flag.clone(),
    );
    let mut operations = WatchOperationScheduler::default();
    if let Err(error) = operations.start(
        operation_context.clone(),
        WatchOperation::Reconcile(ReconcileWork::Startup),
        None,
    ) {
        stop_flag.store(true, Ordering::Relaxed);
        drop(debouncer);
        let _ = socket_thread.join();
        crate::structure::graph::snapshot::forget(repo_root);
        return Err(crate::Error::Other(anyhow::anyhow!(error)));
    }

    let mut pending_auto_sync = false;
    let mut last_reconcile_at = std::time::Instant::now();
    let keepalive_interval = config.reconcile_keepalive_seconds;

    loop {
        if let Some(completion) = operations.reap_finished() {
            handle_completion(
                completion,
                config,
                synrepo_dir,
                &state_handle,
                &mut embedding_refresh,
                &pending_watch,
                &auto_sync_enabled,
                &auto_sync_blocked,
                &mut pending_auto_sync,
                &mut last_reconcile_at,
            );
        }

        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(message) if stop_flag.load(Ordering::Relaxed) => {
                reject_message_while_stopping(message);
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if !operations.is_idle() {
                    continue;
                }

                if branch_ref_poller.maybe_changed(repo_root, config) {
                    if let Ok(mut pending) = pending_watch.lock() {
                        pending.record_full(1);
                    }
                }
                let pending_empty = pending_watch
                    .lock()
                    .map(|pending| pending.is_empty())
                    .unwrap_or(false);

                if !pending_empty {
                    let batch = match pending_watch.lock() {
                        Ok(mut pending) => {
                            let batch = pending.take(watch_config.max_events_per_cycle);
                            pending.clear_paths();
                            batch
                        }
                        Err(_) => continue,
                    };
                    if !start_automatic_operation(
                        &mut operations,
                        operation_context.clone(),
                        WatchOperation::Reconcile(ReconcileWork::Watch {
                            batch,
                            keepalive: false,
                        }),
                        &events,
                    ) {
                        if let Ok(mut pending) = pending_watch.lock() {
                            pending.record_full(1);
                        }
                    }
                    continue;
                }

                if pending_auto_sync
                    && auto_sync_enabled.load(Ordering::Relaxed)
                    && !auto_sync_blocked.load(Ordering::Relaxed)
                {
                    if start_automatic_operation(
                        &mut operations,
                        operation_context.clone(),
                        WatchOperation::Sync {
                            options: Default::default(),
                            trigger: SyncTrigger::AutoPostReconcile,
                        },
                        &events,
                    ) {
                        pending_auto_sync = false;
                    }
                    continue;
                }

                if embedding_refresh.should_start_auto_refresh(
                    config,
                    synrepo_dir,
                    &auto_sync_enabled,
                    &auto_sync_blocked,
                    false,
                ) {
                    let _ = start_automatic_operation(
                        &mut operations,
                        operation_context.clone(),
                        WatchOperation::Embeddings {
                            trigger: EmbeddingTrigger::AutoRefresh,
                        },
                        &events,
                    );
                    continue;
                }

                let keepalive_due = keepalive_interval > 0
                    && last_reconcile_at.elapsed().as_secs() >= keepalive_interval as u64;
                if keepalive_due {
                    let _ = start_automatic_operation(
                        &mut operations,
                        operation_context.clone(),
                        WatchOperation::Reconcile(ReconcileWork::Watch {
                            batch: super::pending::PendingWatchBatch {
                                event_count: 0,
                                touched_paths: Vec::new(),
                                force_full_reconcile: false,
                            },
                            keepalive: true,
                        }),
                        &events,
                    );
                }
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
                start_manual_operation(
                    &mut operations,
                    operation_context.clone(),
                    WatchOperation::Reconcile(ReconcileWork::Manual { fast }),
                    respond_to,
                );
            }
            Ok(LoopMessage::SyncNow {
                respond_to,
                options,
            }) => {
                if start_manual_operation(
                    &mut operations,
                    operation_context.clone(),
                    WatchOperation::Sync {
                        options,
                        trigger: SyncTrigger::Manual,
                    },
                    respond_to,
                ) {
                    pending_auto_sync = false;
                }
            }
            Ok(LoopMessage::EmbeddingsBuildNow { respond_to }) => {
                start_manual_operation(
                    &mut operations,
                    operation_context.clone(),
                    WatchOperation::Embeddings {
                        trigger: EmbeddingTrigger::Manual,
                    },
                    respond_to,
                );
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    stop_flag.store(true, Ordering::Relaxed);
    drop(debouncer);
    operations.reject_active_request_on_stop();
    while let Ok(message) = rx.try_recv() {
        reject_message_while_stopping(message);
    }
    let _ = socket_thread.join();
    operations.shutdown(process_exits_after_return);
    crate::structure::graph::snapshot::forget(repo_root);
    Ok(())
}
