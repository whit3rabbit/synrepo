use std::{
    fs,
    io::Write as _,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::pipeline::writer::{is_process_alive, now_rfc3339};

const WATCH_DAEMON_FILENAME: &str = "watch-daemon.json";
const WATCH_SOCKET_FILENAME: &str = "watch.sock";
static NEXT_WATCH_STATE_TMP_ID: AtomicU64 = AtomicU64::new(0);

/// Foreground or daemon execution mode for the watch service.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchServiceMode {
    /// Running in the current `synrepo watch` process.
    Foreground,
    /// Running in a detached helper process started by `synrepo watch --daemon`.
    Daemon,
}

impl std::fmt::Display for WatchServiceMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Foreground => f.write_str("foreground"),
            Self::Daemon => f.write_str("daemon"),
        }
    }
}

/// Persisted watch-service ownership and telemetry record.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WatchDaemonState {
    /// OS process ID of the service owner.
    pub pid: u32,
    /// RFC 3339 UTC timestamp when the service acquired the lease.
    pub started_at: String,
    /// Whether the service is foreground or detached.
    pub mode: WatchServiceMode,
    /// Canonical path of the control socket for this repo.
    pub socket_path: String,
    /// Most recent filesystem event burst seen outside `.synrepo/`.
    pub last_event_at: Option<String>,
    /// RFC 3339 UTC timestamp of the last completed reconcile attempt.
    pub last_reconcile_at: Option<String>,
    /// Stable outcome string from the last reconcile attempt.
    pub last_reconcile_outcome: Option<String>,
    /// Last reconcile error message, if any.
    pub last_error: Option<String>,
    /// Number of triggering events in the last reconcile burst.
    pub last_triggering_events: Option<usize>,
}

impl WatchDaemonState {
    fn new(synrepo_dir: &Path, mode: WatchServiceMode) -> Self {
        Self {
            pid: std::process::id(),
            started_at: now_rfc3339(),
            mode,
            socket_path: watch_socket_path(synrepo_dir).display().to_string(),
            last_event_at: None,
            last_reconcile_at: None,
            last_reconcile_outcome: None,
            last_error: None,
            last_triggering_events: None,
        }
    }

    fn same_owner(&self, other: &Self) -> bool {
        self.pid == other.pid && self.started_at == other.started_at
    }
}

/// Current watch-service state for a repo.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WatchServiceStatus {
    /// No watch service lease exists.
    Inactive,
    /// A live watch service owns the repo.
    Running(WatchDaemonState),
    /// A stale lease or socket remains from a dead service.
    Stale(Option<WatchDaemonState>),
}

/// Errors raised by the watch daemon lease or control plane.
#[derive(Debug, thiserror::Error)]
pub enum WatchDaemonError {
    /// Another live watch service owns the repo.
    #[error("watch service already running under pid {pid}; stop it before starting another")]
    HeldByOther {
        /// PID of the current owner.
        pid: u32,
        /// Path to the lease file.
        state_path: PathBuf,
    },
    /// I/O error touching the lease or socket.
    #[error("I/O error at {path}: {source}")]
    Io {
        /// Path where the error occurred.
        path: PathBuf,
        #[source]
        /// Underlying I/O error.
        source: std::io::Error,
    },
    /// Control request failed.
    #[error("{0}")]
    Control(String),
}

/// Shared mutable service telemetry.
#[derive(Clone, Debug)]
pub struct WatchStateHandle {
    state_path: PathBuf,
    state: std::sync::Arc<Mutex<WatchDaemonState>>,
}

impl WatchStateHandle {
    pub(crate) fn new(state_path: PathBuf, initial: WatchDaemonState) -> Self {
        Self {
            state_path,
            state: std::sync::Arc::new(Mutex::new(initial)),
        }
    }

    /// Return the latest in-memory snapshot.
    pub fn snapshot(&self) -> WatchDaemonState {
        self.state.lock().clone()
    }

    /// Record that a filesystem burst outside `.synrepo/` was observed.
    pub fn note_event(&self) {
        let snapshot = {
            let mut state = self.state.lock();
            state.last_event_at = Some(now_rfc3339());
            state.clone()
        };
        let _ = persist_watch_state_at(&self.state_path, &snapshot);
    }

    /// Record the latest reconcile outcome.
    pub fn note_reconcile(
        &self,
        outcome: &super::reconcile::ReconcileOutcome,
        triggering_events: usize,
    ) {
        let snapshot = {
            let mut state = self.state.lock();
            state.last_reconcile_at = Some(now_rfc3339());
            state.last_reconcile_outcome = Some(outcome.as_str().to_string());
            state.last_triggering_events = Some(triggering_events);
            state.last_error = match outcome {
                super::reconcile::ReconcileOutcome::Failed(message) => Some(message.clone()),
                _ => None,
            };
            state.clone()
        };
        let _ = persist_watch_state_at(&self.state_path, &snapshot);
    }
}

/// RAII guard holding the per-repo watch-service lease.
#[derive(Debug)]
pub struct WatchDaemonLease {
    state_path: PathBuf,
    socket_path: PathBuf,
    identity: WatchDaemonState,
}

impl Drop for WatchDaemonLease {
    fn drop(&mut self) {
        let current = load_watch_state_from_path(&self.state_path);
        if current
            .as_ref()
            .is_some_and(|state| state.same_owner(&self.identity))
        {
            cleanup_file(&self.state_path);
            cleanup_file(&self.socket_path);
        }
    }
}

/// Acquire the per-repo watch-service lease and write the initial state file.
pub(crate) fn acquire_watch_daemon_lease(
    synrepo_dir: &Path,
    mode: WatchServiceMode,
) -> Result<(WatchDaemonLease, WatchStateHandle), WatchDaemonError> {
    let state_path = watch_daemon_state_path(synrepo_dir);
    let socket_path = watch_socket_path(synrepo_dir);
    let state_dir = synrepo_dir.join("state");
    fs::create_dir_all(&state_dir).map_err(|source| WatchDaemonError::Io {
        path: state_dir.clone(),
        source,
    })?;

    let initial = WatchDaemonState::new(synrepo_dir, mode);
    let json = serde_json::to_string(&initial).expect("WatchDaemonState serializes");
    let mut cleared_stale = false;

    loop {
        match fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&state_path)
        {
            Ok(mut file) => {
                if let Err(source) = file.write_all(json.as_bytes()) {
                    cleanup_file(&state_path);
                    return Err(WatchDaemonError::Io {
                        path: state_path.clone(),
                        source,
                    });
                }
                if let Err(source) = file.sync_all() {
                    cleanup_file(&state_path);
                    return Err(WatchDaemonError::Io {
                        path: state_path.clone(),
                        source,
                    });
                }
                let lease = WatchDaemonLease {
                    state_path: state_path.clone(),
                    socket_path,
                    identity: initial.clone(),
                };
                let handle = WatchStateHandle::new(state_path, initial);
                return Ok((lease, handle));
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if cleared_stale {
                    return Err(
                        match load_watch_state_from_path(&watch_daemon_state_path(synrepo_dir)) {
                            Some(state) => WatchDaemonError::HeldByOther {
                                pid: state.pid,
                                state_path: watch_daemon_state_path(synrepo_dir),
                            },
                            None => WatchDaemonError::Io {
                                path: watch_daemon_state_path(synrepo_dir),
                                source: error,
                            },
                        },
                    );
                }

                match load_watch_state_with_retry(&watch_daemon_state_path(synrepo_dir)) {
                    Some(state) if is_process_alive(state.pid) => {
                        return Err(WatchDaemonError::HeldByOther {
                            pid: state.pid,
                            state_path: watch_daemon_state_path(synrepo_dir),
                        });
                    }
                    _ => {
                        cleanup_file(&watch_daemon_state_path(synrepo_dir));
                        cleanup_file(&watch_socket_path(synrepo_dir));
                        cleared_stale = true;
                    }
                }
            }
            Err(source) => {
                return Err(WatchDaemonError::Io {
                    path: watch_daemon_state_path(synrepo_dir),
                    source,
                });
            }
        }
    }
}

/// Load the persisted watch-service state, if present and readable.
pub fn load_watch_state(synrepo_dir: &Path) -> Option<WatchDaemonState> {
    load_watch_state_from_path(&watch_daemon_state_path(synrepo_dir))
}

/// Inspect whether the repo currently has a live, stale, or missing watch service.
pub fn watch_service_status(synrepo_dir: &Path) -> WatchServiceStatus {
    let state_path = watch_daemon_state_path(synrepo_dir);
    if !state_path.exists() {
        return WatchServiceStatus::Inactive;
    }

    match load_watch_state_from_path(&state_path) {
        Some(state) if is_process_alive(state.pid) => WatchServiceStatus::Running(state),
        Some(state) => WatchServiceStatus::Stale(Some(state)),
        None => WatchServiceStatus::Stale(None),
    }
}

/// Remove stale watch-service artifacts left behind by a dead process.
pub fn cleanup_stale_watch_artifacts(synrepo_dir: &Path) -> Result<bool, WatchDaemonError> {
    match watch_service_status(synrepo_dir) {
        WatchServiceStatus::Inactive | WatchServiceStatus::Running(_) => Ok(false),
        WatchServiceStatus::Stale(_) => {
            let state_path = watch_daemon_state_path(synrepo_dir);
            let socket_path = watch_socket_path(synrepo_dir);
            if let Err(source) = fs::remove_file(&state_path) {
                if source.kind() != std::io::ErrorKind::NotFound {
                    return Err(WatchDaemonError::Io {
                        path: state_path,
                        source,
                    });
                }
            }
            if let Err(source) = fs::remove_file(&socket_path) {
                if source.kind() != std::io::ErrorKind::NotFound {
                    return Err(WatchDaemonError::Io {
                        path: socket_path,
                        source,
                    });
                }
            }
            Ok(true)
        }
    }
}

/// Canonical path of the persisted watch-service state file.
pub fn watch_daemon_state_path(synrepo_dir: &Path) -> PathBuf {
    synrepo_dir.join("state").join(WATCH_DAEMON_FILENAME)
}

/// Canonical path of the per-repo watch control socket.
pub fn watch_socket_path(synrepo_dir: &Path) -> PathBuf {
    synrepo_dir.join("state").join(WATCH_SOCKET_FILENAME)
}

pub(super) fn persist_watch_state_at(
    state_path: &Path,
    state: &WatchDaemonState,
) -> Result<(), WatchDaemonError> {
    let json = serde_json::to_string(state).expect("WatchDaemonState serializes");
    let state_dir = state_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let tmp_path = watch_state_tmp_path(&state_dir);
    fs::write(&tmp_path, json.as_bytes())
        .and_then(|_| fs::rename(&tmp_path, state_path))
        .map_err(|source| WatchDaemonError::Io {
            path: state_path.to_path_buf(),
            source,
        })?;
    Ok(())
}

fn load_watch_state_from_path(state_path: &Path) -> Option<WatchDaemonState> {
    let text = fs::read_to_string(state_path).ok()?;
    serde_json::from_str(&text).ok()
}

fn load_watch_state_with_retry(state_path: &Path) -> Option<WatchDaemonState> {
    const RETRIES: u32 = 5;
    const BACKOFF_MS: u64 = 1;
    for _ in 0..RETRIES {
        if let Some(state) = load_watch_state_from_path(state_path) {
            return Some(state);
        }
        std::thread::sleep(Duration::from_millis(BACKOFF_MS));
    }
    load_watch_state_from_path(state_path)
}

fn watch_state_tmp_path(state_dir: &Path) -> PathBuf {
    let id = NEXT_WATCH_STATE_TMP_ID.fetch_add(1, Ordering::Relaxed);
    state_dir.join(format!(
        "{WATCH_DAEMON_FILENAME}.tmp.{}.{}",
        std::process::id(),
        id
    ))
}

fn cleanup_file(path: &Path) {
    if let Err(error) = fs::remove_file(path) {
        if error.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(path = ?path, error = %error, "failed to clean up watch artifact");
        }
    }
}
