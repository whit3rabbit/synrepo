use std::{
    fs,
    path::{Path, PathBuf},
};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use super::paths::watch_control_endpoint;
use crate::pipeline::writer::now_rfc3339;

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
    /// Platform-specific control endpoint identifier (socket filesystem path
    /// on Unix, named-pipe name on Windows).
    pub control_endpoint: String,
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
    /// Runtime auto-sync flag currently honored by the watch service.
    #[serde(default)]
    pub auto_sync_enabled: bool,
    /// True while the watch loop is running the post-reconcile sync hook.
    #[serde(default)]
    pub auto_sync_running: bool,
    /// True after auto-sync hits a blocked cheap-surface repair.
    #[serde(default)]
    pub auto_sync_paused: bool,
    /// RFC 3339 UTC timestamp of the last auto-sync start.
    #[serde(default)]
    pub auto_sync_last_started_at: Option<String>,
    /// RFC 3339 UTC timestamp of the last auto-sync finish.
    #[serde(default)]
    pub auto_sync_last_finished_at: Option<String>,
    /// Stable outcome string for the last auto-sync or recovery sync.
    #[serde(default)]
    pub auto_sync_last_outcome: Option<String>,
    /// True when the existing embedding index is known to lag source changes.
    #[serde(default)]
    pub embedding_index_stale: bool,
    /// True while the watch loop is building or refreshing embeddings.
    #[serde(default)]
    pub embedding_running: bool,
    /// RFC 3339 UTC timestamp of the last embedding job start.
    #[serde(default)]
    pub embedding_last_started_at: Option<String>,
    /// RFC 3339 UTC timestamp of the last embedding job finish.
    #[serde(default)]
    pub embedding_last_finished_at: Option<String>,
    /// Stable outcome string for the last embedding job.
    #[serde(default)]
    pub embedding_last_outcome: Option<String>,
    /// Last embedding error message, if any.
    #[serde(default)]
    pub embedding_last_error: Option<String>,
    /// Current embedding progress phase.
    #[serde(default)]
    pub embedding_progress_phase: Option<String>,
    /// Current embedded chunk count for the running job.
    #[serde(default)]
    pub embedding_progress_current: Option<usize>,
    /// Total chunks planned for the running job.
    #[serde(default)]
    pub embedding_progress_total: Option<usize>,
    /// RFC 3339 UTC timestamp when automatic embedding refresh may retry.
    #[serde(default)]
    pub embedding_next_retry_at: Option<String>,
}

impl WatchDaemonState {
    pub(crate) fn new(synrepo_dir: &Path, mode: WatchServiceMode) -> Self {
        Self {
            pid: std::process::id(),
            started_at: now_rfc3339(),
            mode,
            control_endpoint: watch_control_endpoint(synrepo_dir),
            last_event_at: None,
            last_reconcile_at: None,
            last_reconcile_outcome: None,
            last_error: None,
            last_triggering_events: None,
            auto_sync_enabled: false,
            auto_sync_running: false,
            auto_sync_paused: false,
            auto_sync_last_started_at: None,
            auto_sync_last_finished_at: None,
            auto_sync_last_outcome: None,
            embedding_index_stale: false,
            embedding_running: false,
            embedding_last_started_at: None,
            embedding_last_finished_at: None,
            embedding_last_outcome: None,
            embedding_last_error: None,
            embedding_progress_phase: None,
            embedding_progress_current: None,
            embedding_progress_total: None,
            embedding_next_retry_at: None,
        }
    }

    pub(crate) fn same_owner(&self, other: &Self) -> bool {
        self.pid == other.pid && self.started_at == other.started_at
    }
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
    /// Another live watch service holds the lease but has not published state yet.
    #[error("watch service is starting for this repo; wait for it to become ready or stop it before starting another")]
    HeldByStarting {
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
    /// A security violation was detected (e.g. symlink or foreign-owned socket dir).
    #[error("Security violation: {0}")]
    Security(String),
    /// Control request failed.
    #[error("{0}")]
    Control(String),
}

/// Shared mutable service telemetry.
#[derive(Clone, Debug)]
pub struct WatchStateHandle {
    pub(crate) state_path: PathBuf,
    pub(crate) state: std::sync::Arc<Mutex<WatchDaemonState>>,
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

    pub fn note_auto_sync_enabled(&self, enabled: bool) {
        let snapshot = {
            let mut state = self.state.lock();
            state.auto_sync_enabled = enabled;
            if enabled {
                state.auto_sync_paused = false;
            } else {
                state.auto_sync_running = false;
            }
            state.clone()
        };
        let _ = super::persist_watch_state_at(&self.state_path, &snapshot);
    }

    pub fn note_auto_sync_started(&self) {
        let snapshot = {
            let mut state = self.state.lock();
            state.auto_sync_running = true;
            state.auto_sync_paused = false;
            state.auto_sync_last_started_at = Some(now_rfc3339());
            state.auto_sync_last_outcome = Some("running".to_string());
            state.clone()
        };
        let _ = super::persist_watch_state_at(&self.state_path, &snapshot);
    }

    pub fn note_auto_sync_finished(&self, blocked: bool) {
        let snapshot = {
            let mut state = self.state.lock();
            state.auto_sync_running = false;
            state.auto_sync_paused = blocked;
            state.auto_sync_last_finished_at = Some(now_rfc3339());
            state.auto_sync_last_outcome =
                Some(if blocked { "blocked" } else { "completed" }.to_string());
            state.clone()
        };
        let _ = super::persist_watch_state_at(&self.state_path, &snapshot);
    }

    pub fn note_auto_sync_error(&self, message: impl Into<String>) {
        let snapshot = {
            let mut state = self.state.lock();
            state.auto_sync_running = false;
            state.auto_sync_last_finished_at = Some(now_rfc3339());
            state.auto_sync_last_outcome = Some(format!("error: {}", message.into()));
            state.clone()
        };
        let _ = super::persist_watch_state_at(&self.state_path, &snapshot);
    }

    pub fn note_manual_sync_finished(&self, blocked: bool) {
        if blocked {
            return;
        }
        let snapshot = {
            let mut state = self.state.lock();
            state.auto_sync_paused = false;
            state.auto_sync_last_finished_at = Some(now_rfc3339());
            state.auto_sync_last_outcome = Some("manual_sync_completed".to_string());
            state.clone()
        };
        let _ = super::persist_watch_state_at(&self.state_path, &snapshot);
    }

    /// Record that a filesystem burst outside `.synrepo/` was observed.
    pub fn note_event(&self) {
        let snapshot = {
            let mut state = self.state.lock();
            state.last_event_at = Some(now_rfc3339());
            state.clone()
        };
        let _ = super::persist_watch_state_at(&self.state_path, &snapshot);
    }

    /// Record the latest reconcile outcome.
    pub fn note_reconcile(
        &self,
        outcome: &super::super::reconcile::ReconcileOutcome,
        triggering_events: usize,
    ) {
        let snapshot = {
            let mut state = self.state.lock();
            state.last_reconcile_at = Some(now_rfc3339());
            state.last_reconcile_outcome = Some(outcome.as_str().to_string());
            state.last_triggering_events = Some(triggering_events);
            state.last_error = match outcome {
                super::super::reconcile::ReconcileOutcome::Failed(message) => Some(message.clone()),
                _ => None,
            };
            state.clone()
        };
        let _ = super::persist_watch_state_at(&self.state_path, &snapshot);
    }
}

/// RAII guard holding the per-repo watch-service lease.
#[derive(Debug)]
pub struct WatchDaemonLease {
    pub(crate) state_path: PathBuf,
    pub(crate) flock_path: PathBuf,
    pub(crate) socket_path: PathBuf,
    pub(crate) identity: WatchDaemonState,
    /// Held for its Drop side-effect (releases the kernel flock).
    #[allow(dead_code)]
    pub(crate) flock_file: fs::File,
}

impl Drop for WatchDaemonLease {
    fn drop(&mut self) {
        let current = super::load_watch_state_from_path(&self.state_path);
        if current
            .as_ref()
            .is_ok_and(|state| state.same_owner(&self.identity))
        {
            super::cleanup_file(&self.state_path);
            super::cleanup_file(&self.flock_path);
            super::cleanup_file(&self.socket_path);
        }
    }
}

/// Guard returned by [`hold_watch_flock_with_state`]. Dropping it releases
/// the kernel advisory lock held on its file descriptor.
#[doc(hidden)]
pub struct TestWatchFlockHolder {
    pub(crate) _file: fs::File,
}
