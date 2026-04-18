// The watch service uses Unix sockets and is only active on unix platforms.
// On Windows the top-level function returns an unsupported error immediately,
// leaving all service/lease helpers as dead code.  Suppress the lint rather
// than gating every item individually.
#![cfg_attr(not(unix), allow(dead_code, unused_imports))]

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::Duration,
};

use notify_debouncer_full::{
    new_debouncer,
    notify::{RecursiveMode, Watcher},
    DebounceEventResult, DebouncedEvent,
};

use crate::config::Config;

#[cfg(unix)]
use super::control::{read_control_request, write_control_response};
use super::{
    control::{WatchControlRequest, WatchControlResponse},
    lease::{acquire_watch_daemon_lease, watch_socket_path, WatchServiceMode, WatchStateHandle},
    reconcile::{persist_reconcile_state, run_reconcile_pass, ReconcileOutcome},
};

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

enum LoopMessage {
    WatchResult(DebounceEventResult),
    Stop {
        respond_to: mpsc::Sender<WatchControlResponse>,
    },
    ReconcileNow {
        respond_to: mpsc::Sender<WatchControlResponse>,
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
    #[cfg(not(unix))]
    {
        let _ = (repo_root, config, watch_config, synrepo_dir, mode, events);
        Err(crate::Error::Other(anyhow::anyhow!(
            "watch daemon service is only supported on unix-like platforms"
        )))
    }

    #[cfg(unix)]
    {
        let (_lease, state_handle) = acquire_watch_daemon_lease(synrepo_dir, mode)
            .map_err(|error| crate::Error::Other(anyhow::anyhow!(error.to_string())))?;

        let stop_flag = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel::<LoopMessage>();
        let socket_thread = spawn_control_listener(
            synrepo_dir,
            state_handle.clone(),
            tx.clone(),
            stop_flag.clone(),
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

        let mut debouncer = new_debouncer(watch_config.debounce_timeout, None, move |result| {
            let _ = tx.send(LoopMessage::WatchResult(result));
        })
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
            match rx.recv() {
                Ok(LoopMessage::WatchResult(Ok(watcher_events))) => {
                    let filtered = filter_repo_events(watcher_events, synrepo_dir);
                    if filtered.is_empty() {
                        continue;
                    }
                    let event_count = filtered.len().min(watch_config.max_events_per_cycle);
                    state_handle.note_event();
                    tracing::debug!(events = event_count, "coalesced events; running reconcile");
                    emit_event(&events, |now| WatchEvent::ReconcileStarted {
                        at: now,
                        triggering_events: event_count,
                    });
                    let outcome = run_reconcile_pass(repo_root, config, synrepo_dir);
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
                }
                Ok(LoopMessage::WatchResult(Err(errors))) => {
                    for error in &errors {
                        tracing::warn!("watcher error: {error}");
                        emit_event(&events, |now| WatchEvent::Error {
                            at: now,
                            message: format!("watcher error: {error}"),
                        });
                    }
                }
                Ok(LoopMessage::Stop { respond_to }) => {
                    stop_flag.store(true, Ordering::Relaxed);
                    let _ = respond_to.send(WatchControlResponse::Ack {
                        message: "watch service stopping".to_string(),
                    });
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
                    let _ = respond_to.send(WatchControlResponse::Reconcile {
                        outcome,
                        triggering_events: 0,
                    });
                }
                Err(_) => break,
            }
        }

        stop_flag.store(true, Ordering::Relaxed);
        drop(debouncer);
        let _ = socket_thread.join();
        Ok(())
    }
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

pub(crate) fn filter_repo_events(
    events: Vec<DebouncedEvent>,
    synrepo_dir: &Path,
) -> Vec<DebouncedEvent> {
    let canonical_synrepo_dir = canonicalize_lossy(synrepo_dir);
    events
        .into_iter()
        .filter(|event| {
            !event.paths.iter().all(|path| {
                path_matches_runtime(path, synrepo_dir, canonical_synrepo_dir.as_deref())
            })
        })
        .collect()
}

fn path_matches_runtime(
    path: &Path,
    synrepo_dir: &Path,
    canonical_synrepo_dir: Option<&Path>,
) -> bool {
    if path.starts_with(synrepo_dir) || synrepo_dir.starts_with(path) {
        return true;
    }

    match (canonicalize_lossy(path), canonical_synrepo_dir) {
        (Some(canonical_path), Some(canonical_synrepo_dir)) => {
            canonical_path.starts_with(canonical_synrepo_dir)
                || canonical_synrepo_dir.starts_with(&canonical_path)
        }
        _ => false,
    }
}

fn canonicalize_lossy(path: &Path) -> Option<PathBuf> {
    fs::canonicalize(path).ok().or_else(|| {
        let name = path.file_name()?;
        let parent = path.parent()?;
        let canonical_parent = fs::canonicalize(parent).ok()?;
        Some(canonical_parent.join(name))
    })
}

#[cfg(unix)]
fn spawn_control_listener(
    synrepo_dir: &Path,
    state_handle: WatchStateHandle,
    tx: mpsc::Sender<LoopMessage>,
    stop_flag: Arc<AtomicBool>,
) -> crate::Result<thread::JoinHandle<()>> {
    use std::os::unix::net::UnixListener;

    let socket_path = watch_socket_path(synrepo_dir);
    if socket_path.exists() {
        let _ = std::fs::remove_file(&socket_path);
    }

    let listener = UnixListener::bind(&socket_path).map_err(|error| {
        crate::Error::Other(anyhow::anyhow!(
            "failed to bind watch control socket {}: {error}",
            socket_path.display()
        ))
    })?;
    listener.set_nonblocking(true).map_err(|error| {
        crate::Error::Other(anyhow::anyhow!(
            "failed to configure watch control socket {}: {error}",
            socket_path.display()
        ))
    })?;

    Ok(thread::spawn(move || {
        while !stop_flag.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _addr)) => {
                    let response = match read_control_request(&mut stream) {
                        Ok(WatchControlRequest::Status) => WatchControlResponse::Status {
                            snapshot: state_handle.snapshot(),
                        },
                        Ok(WatchControlRequest::Stop) => bridge_stop_request(&tx),
                        Ok(WatchControlRequest::ReconcileNow) => bridge_reconcile_request(&tx),
                        Err(error) => WatchControlResponse::Error {
                            message: error.to_string(),
                        },
                    };
                    if let Err(error) = write_control_response(&mut stream, &response) {
                        tracing::warn!(error = %error, "failed to write watch control response");
                    }
                }
                // REVIEW NOTE: the 50ms sleep is the backoff for the
                // non-blocking accept loop. Removing it pegs a CPU core at
                // 100% while the daemon idles. Keep this arm.
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

#[cfg(unix)]
fn bridge_stop_request(tx: &mpsc::Sender<LoopMessage>) -> WatchControlResponse {
    let (respond_to, recv_from_loop) = mpsc::channel();
    if tx.send(LoopMessage::Stop { respond_to }).is_err() {
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

#[cfg(unix)]
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
