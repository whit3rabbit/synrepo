// The watch service uses `interprocess::local_socket` for its control plane:
// Unix domain sockets on Unix, Windows named pipes on Windows. One blocking
// protocol implementation backs both.

use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    time::Duration,
};

use notify_debouncer_full::{new_debouncer, notify::RecursiveMode};

use crate::{
    config::Config,
    pipeline::repair::{SyncOptions, SyncProgress, SyncSummary},
};

pub(super) use super::filter::filter_repo_events;
use super::{
    control::WatchControlResponse,
    control_bridge::spawn_control_listener,
    filter::{collect_repo_paths, ignored_generated_dirs},
    lease::{acquire_watch_daemon_lease, WatchServiceMode},
    pending::PendingWatchChanges,
    reconcile::{
        persist_reconcile_state, run_reconcile_pass, run_reconcile_pass_with_touched_paths,
        ReconcileOutcome,
    },
    sync::{emit_event, maybe_run_post_reconcile_auto_sync, run_sync_under_watch_lock},
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
        fast: bool,
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
    let startup = run_reconcile_pass(repo_root, config, synrepo_dir, false);
    persist_reconcile_state(synrepo_dir, &startup, 0);
    state_handle.note_reconcile(&startup, 0);
    tracing::info!(outcome = %startup.as_str(), "startup reconcile complete");
    emit_event(&events, |now| WatchEvent::ReconcileFinished {
        at: now,
        outcome: startup.clone(),
        triggering_events: 0,
    });

    let pending_watch = Arc::new(Mutex::new(PendingWatchChanges::default()));
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
                    let touched_paths = collect_repo_paths(
                        &filtered,
                        &callback_repo_roots,
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

    maybe_run_post_reconcile_auto_sync(
        repo_root,
        config,
        synrepo_dir,
        &startup,
        &auto_sync_enabled,
        &auto_sync_blocked,
        &events,
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
                emit_event(&events, |now| WatchEvent::ReconcileStarted {
                    at: now,
                    triggering_events: event_count,
                });
                // Keepalive runs fast (skip git-history passes) since FS state
                // is already being observed by the debouncer; the goal is to
                // refresh the timestamp and auto-sync hook, not re-mine git.
                let outcome = run_reconcile_pass_with_touched_paths(
                    repo_root,
                    config,
                    synrepo_dir,
                    (!batch.touched_paths.is_empty()).then_some(batch.touched_paths.as_slice()),
                    keepalive,
                );
                persist_reconcile_state(synrepo_dir, &outcome, event_count);
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
            Ok(LoopMessage::ReconcileNow { respond_to, fast }) => {
                emit_event(&events, |now| WatchEvent::ReconcileStarted {
                    at: now,
                    triggering_events: 0,
                });
                let outcome = run_reconcile_pass(repo_root, config, synrepo_dir, fast);
                persist_reconcile_state(synrepo_dir, &outcome, 0);
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
