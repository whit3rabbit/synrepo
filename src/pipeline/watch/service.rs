// The watch service uses `interprocess::local_socket` for its control plane:
// Unix domain sockets on Unix, Windows named pipes on Windows. One blocking
// protocol implementation backs both.

#[cfg(unix)]
use std::fs;
use std::{
    io::BufReader,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::Duration,
};

use interprocess::local_socket::{traits::Listener as _, ListenerNonblockingMode, ListenerOptions};
use notify_debouncer_full::{
    new_debouncer,
    notify::{RecursiveMode, Watcher},
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

pub(super) use super::filter::filter_repo_events;
use super::{
    control::{
        read_control_request, write_control_response, WatchControlRequest, WatchControlResponse,
    },
    filter::{collect_repo_paths, ignored_generated_dirs},
    lease::{
        acquire_watch_daemon_lease, watch_control_endpoint, watch_control_socket_name,
        WatchServiceMode, WatchStateHandle,
    },
    pending::PendingWatchChanges,
    reconcile::{
        persist_reconcile_state, run_reconcile_pass, run_reconcile_pass_with_touched_paths,
        ReconcileOutcome,
    },
};

/// Why a sync pass is running inside the watch service.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncTrigger {
    /// CLI sent `SyncNow` over the control socket, or the TUI pressed `S`.
    Manual,
    /// The reconcile loop opted into auto-sync for cheap surfaces.
    AutoPostReconcile,
}

/// Event emitted by the watch service for each reconcile attempt and error.
///
/// Used by the live-mode dashboard to stream activity into the log pane.
/// The wire format is intentionally minimal: keep this structure close to the
/// poll dashboard's `LogEntry` shape so mapping stays trivial.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum WatchEvent {
    /// Emitted immediately before `run_reconcile_pass` runs. `triggering_events`
    /// is 0 for the startup pass and for operator-requested passes.
    ReconcileStarted {
        /// RFC 3339 UTC timestamp when the pass started.
        at: String,
        /// Number of debounced filesystem events that triggered this pass.
        triggering_events: usize,
    },
    /// Emitted after a reconcile pass completes with its outcome.
    ReconcileFinished {
        /// RFC 3339 UTC timestamp when the pass finished.
        at: String,
        /// Final outcome from `run_reconcile_pass`.
        outcome: ReconcileOutcome,
        /// Number of debounced filesystem events that triggered this pass.
        triggering_events: usize,
    },
    /// Emitted before a repair sync pass runs inside the watch service.
    SyncStarted {
        /// RFC 3339 UTC timestamp when the pass started.
        at: String,
        /// Whether this is an operator-requested or auto-triggered sync.
        trigger: SyncTrigger,
    },
    /// Emitted for each surface boundary and commentary sub-event during sync.
    SyncProgress {
        /// RFC 3339 UTC timestamp when the progress event was emitted.
        at: String,
        /// The structured progress payload.
        progress: SyncProgress,
    },
    /// Emitted when a sync pass finishes, with the resulting summary.
    SyncFinished {
        /// RFC 3339 UTC timestamp when the pass finished.
        at: String,
        /// Why the sync ran.
        trigger: SyncTrigger,
        /// Completed summary (empty vectors if no findings).
        summary: SyncSummary,
    },
    /// Emitted for watcher-level errors (debouncer failures). Reconcile
    /// failures surface as `ReconcileFinished { outcome: Failed(_) }`.
    Error {
        /// RFC 3339 UTC timestamp when the error was observed.
        at: String,
        /// Human-readable error description.
        message: String,
    },
}

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

pub(super) enum LoopMessage {
    Stop,
    ReconcileNow {
        respond_to: mpsc::Sender<WatchControlResponse>,
    },
    SyncNow {
        respond_to: mpsc::Sender<WatchControlResponse>,
        options: SyncOptions,
    },
}

/// Run the watch service in the current process.
///
/// `events` is an optional best-effort event stream. The live-mode dashboard
/// subscribes here; foreground and daemon callers pass `None`. A dropped
/// receiver does not stop the loop — sends are best-effort.
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
    let (tx, rx) = mpsc::channel::<LoopMessage>();
    let socket_thread = spawn_control_listener(
        synrepo_dir,
        state_handle.clone(),
        tx.clone(),
        stop_flag.clone(),
        auto_sync_enabled.clone(),
    )?;

    emit_event(&events, |now| WatchEvent::ReconcileStarted {
        at: now,
        triggering_events: 0,
    });
    let startup = run_reconcile_pass(repo_root, config, synrepo_dir);
    persist_reconcile_state(synrepo_dir, &startup, 0);
    state_handle.note_reconcile(&startup, 0);
    tracing::info!(outcome = %startup.as_str(), "startup reconcile complete");
    emit_event(&events, |now| WatchEvent::ReconcileFinished {
        at: now,
        outcome: startup.clone(),
        triggering_events: 0,
    });

    let pending_watch = Arc::new(Mutex::new(PendingWatchChanges::default()));
    let callback_repo_root = repo_root.to_path_buf();
    let callback_synrepo_dir = synrepo_dir.to_path_buf();
    let callback_ignored_dirs = ignored_generated_dirs(repo_root, config);
    let callback_state_handle = state_handle.clone();
    let callback_events = events.clone();
    let pending_watch_for_callback = pending_watch.clone();
    let max_events_per_cycle = watch_config.max_events_per_cycle;
    let mut debouncer =
        new_debouncer(
            watch_config.debounce_timeout,
            None,
            move |result| match result {
                Ok(watcher_events) => {
                    let filtered = filter_repo_events(
                        watcher_events,
                        &callback_synrepo_dir,
                        &callback_ignored_dirs,
                    );
                    if filtered.is_empty() {
                        return;
                    }
                    let touched_paths = collect_repo_paths(
                        &filtered,
                        &callback_repo_root,
                        &callback_synrepo_dir,
                        &callback_ignored_dirs,
                    );
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

    debouncer
        .watcher()
        .watch(repo_root, RecursiveMode::Recursive)
        .map_err(|error| {
            crate::Error::Other(anyhow::anyhow!(
                "failed to watch {}: {error}",
                repo_root.display()
            ))
        })?;

    loop {
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(message) if stop_flag.load(Ordering::Relaxed) => match message {
                LoopMessage::Stop => break,
                _ => break,
            },
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let batch = match pending_watch.lock() {
                    Ok(mut pending) => {
                        if pending.is_empty() {
                            continue;
                        }
                        let batch = pending.take(watch_config.max_events_per_cycle);
                        pending.clear_paths();
                        batch
                    }
                    Err(_) => continue,
                };
                let event_count = batch.event_count;
                if event_count == 0 {
                    continue;
                }
                tracing::debug!(events = event_count, "coalesced events; running reconcile");
                emit_event(&events, |now| WatchEvent::ReconcileStarted {
                    at: now,
                    triggering_events: event_count,
                });
                let outcome = run_reconcile_pass_with_touched_paths(
                    repo_root,
                    config,
                    synrepo_dir,
                    (!batch.touched_paths.is_empty()).then_some(batch.touched_paths.as_slice()),
                );
                persist_reconcile_state(synrepo_dir, &outcome, event_count);
                state_handle.note_reconcile(&outcome, event_count);
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
                maybe_run_post_reconcile_auto_sync(
                    repo_root,
                    config,
                    synrepo_dir,
                    &outcome,
                    &auto_sync_enabled,
                    &auto_sync_blocked,
                    &events,
                );
            }
            Ok(LoopMessage::Stop) => {
                break;
            }
            Ok(LoopMessage::ReconcileNow { respond_to }) => {
                emit_event(&events, |now| WatchEvent::ReconcileStarted {
                    at: now,
                    triggering_events: 0,
                });
                let outcome = run_reconcile_pass(repo_root, config, synrepo_dir);
                persist_reconcile_state(synrepo_dir, &outcome, 0);
                state_handle.note_reconcile(&outcome, 0);
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
                    repo_root,
                    config,
                    synrepo_dir,
                    &outcome,
                    &auto_sync_enabled,
                    &auto_sync_blocked,
                    &events,
                );
            }
            Ok(LoopMessage::SyncNow {
                respond_to,
                options,
            }) => {
                let response = run_sync_under_watch_lock(
                    repo_root,
                    config,
                    synrepo_dir,
                    options,
                    None,
                    SyncTrigger::Manual,
                    &events,
                );
                let _ = respond_to.send(response);
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    stop_flag.store(true, Ordering::Relaxed);
    drop(debouncer);
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

/// Best-effort send on the optional event channel. A dropped receiver must
/// not kill the watch loop, so failures are swallowed.
fn emit_event<F>(sender: &Option<crossbeam_channel::Sender<WatchEvent>>, build: F)
where
    F: FnOnce(String) -> WatchEvent,
{
    if let Some(tx) = sender {
        let event = build(crate::pipeline::writer::now_rfc3339());
        let _ = tx.try_send(event);
    }
}
fn spawn_control_listener(
    synrepo_dir: &Path,
    state_handle: WatchStateHandle,
    tx: mpsc::Sender<LoopMessage>,
    stop_flag: Arc<AtomicBool>,
    auto_sync_enabled: Arc<AtomicBool>,
) -> crate::Result<thread::JoinHandle<()>> {
    let endpoint = watch_control_endpoint(synrepo_dir);

    // On Unix the backing store is a filesystem path. A prior dead daemon
    // may have left a stale socket file behind, which makes `bind()` fail
    // with EADDRINUSE. Remove it proactively. Windows named pipes are
    // ephemeral and need no pre-cleanup.
    #[cfg(unix)]
    {
        let _ = fs::remove_file(&endpoint);
    }

    let name = watch_control_socket_name(&endpoint).map_err(|error| {
        crate::Error::Other(anyhow::anyhow!(
            "failed to build watch control socket name {endpoint}: {error}"
        ))
    })?;
    let listener = ListenerOptions::new()
        .name(name)
        .nonblocking(ListenerNonblockingMode::Accept)
        .create_sync()
        .map_err(|error| {
            crate::Error::Other(anyhow::anyhow!(
                "failed to bind watch control socket {endpoint}: {error}"
            ))
        })?;

    Ok(thread::spawn(move || {
        while !stop_flag.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok(stream) => {
                    let mut reader = BufReader::new(&stream);
                    let request_result = read_control_request(&mut reader, &endpoint);
                    let response = match request_result {
                        Ok(WatchControlRequest::Status) => WatchControlResponse::Status {
                            snapshot: state_handle.snapshot(),
                        },
                        Ok(WatchControlRequest::Stop) => bridge_stop_request(&tx, &stop_flag),
                        Ok(WatchControlRequest::ReconcileNow) => bridge_reconcile_request(&tx),
                        Ok(WatchControlRequest::SyncNow { options }) => {
                            bridge_sync_request(&tx, options)
                        }
                        Ok(WatchControlRequest::SetAutoSync { enabled }) => {
                            auto_sync_enabled.store(enabled, Ordering::Relaxed);
                            WatchControlResponse::Ack {
                                message: format!(
                                    "auto-sync {}",
                                    if enabled { "on" } else { "off" }
                                ),
                            }
                        }
                        Err(error) => WatchControlResponse::Error {
                            message: error.to_string(),
                        },
                    };
                    if let Err(error) = write_control_response(&stream, &endpoint, &response) {
                        tracing::warn!(error = %error, "failed to write watch control response");
                    }
                }
                // Backoff for the non-blocking accept loop: without this the
                // idle daemon pegs a CPU core at 100%.
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(50));
                }
                Err(error) => {
                    tracing::warn!(error = %error, "watch control listener error");
                    thread::sleep(Duration::from_millis(50));
                }
            }
        }
    }))
}

pub(super) fn bridge_stop_request(
    tx: &mpsc::Sender<LoopMessage>,
    stop_flag: &AtomicBool,
) -> WatchControlResponse {
    stop_flag.store(true, Ordering::Relaxed);
    if tx.send(LoopMessage::Stop).is_err() {
        return WatchControlResponse::Error {
            message: "watch loop is no longer accepting control messages".to_string(),
        };
    }
    WatchControlResponse::Ack {
        message: "watch service stopping".to_string(),
    }
}

fn bridge_reconcile_request(tx: &mpsc::Sender<LoopMessage>) -> WatchControlResponse {
    let (respond_to, recv_from_loop) = mpsc::channel();
    if tx.send(LoopMessage::ReconcileNow { respond_to }).is_err() {
        return WatchControlResponse::Error {
            message: "watch loop is no longer accepting control messages".to_string(),
        };
    }

    recv_from_loop
        .recv_timeout(Duration::from_secs(30))
        .unwrap_or_else(|_| WatchControlResponse::Error {
            message: "watch loop did not answer the control request in time".to_string(),
        })
}

fn bridge_sync_request(
    tx: &mpsc::Sender<LoopMessage>,
    options: SyncOptions,
) -> WatchControlResponse {
    let (respond_to, recv_from_loop) = mpsc::channel();
    if tx
        .send(LoopMessage::SyncNow {
            respond_to,
            options,
        })
        .is_err()
    {
        return WatchControlResponse::Error {
            message: "watch loop is no longer accepting control messages".to_string(),
        };
    }

    // Sync may invoke LLM-backed commentary refresh; allow a longer wait than
    // reconcile's 30 seconds. A wedged loop still surfaces eventually.
    recv_from_loop
        .recv_timeout(Duration::from_secs(600))
        .unwrap_or_else(|_| WatchControlResponse::Error {
            message: "watch loop did not answer the sync request in time".to_string(),
        })
}

/// Acquire the raw writer lock and run one sync pass inline. Runs on the
/// watch main-loop thread. Emits `SyncStarted`/`SyncProgress`/`SyncFinished`
/// events and returns the appropriate `WatchControlResponse`.
fn run_sync_under_watch_lock(
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
fn maybe_run_post_reconcile_auto_sync(
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
