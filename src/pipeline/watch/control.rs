use std::{
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
};

use interprocess::local_socket::{traits::Stream as _, Stream};
use serde::{Deserialize, Serialize};

use crate::pipeline::repair::{SyncOptions, SyncSummary};

use super::lease::{
    watch_control_endpoint, watch_control_socket_name, WatchDaemonError, WatchDaemonState,
};
use super::reconcile::ReconcileOutcome;
use super::status::load_watch_state;

/// Control message sent over the per-repo watch control endpoint.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum WatchControlRequest {
    /// Return the current service snapshot.
    Status,
    /// Stop the service and release the lease.
    Stop,
    /// Run one reconcile pass immediately.
    ReconcileNow {
        /// Skip git-intensive stages.
        fast: bool,
    },
    /// Suppress watcher-triggered reconcile for a short list of paths.
    SuppressPaths {
        /// Absolute source or temporary paths to ignore while the TTL is live.
        paths: Vec<PathBuf>,
        /// Suppression lifetime in milliseconds.
        ttl_ms: u64,
    },
    /// Run one repair sync pass immediately, under the watch's writer lock.
    SyncNow {
        /// Sync-level options (cross-link generation toggles). Surface-level
        /// filtering is not exposed over the control plane; callers that need
        /// it use the in-process entry point directly.
        options: SyncOptions,
    },
    /// Flip the in-memory auto-sync flag. Does not write to `config.toml`.
    SetAutoSync {
        /// Desired runtime state.
        enabled: bool,
    },
}

/// Control response returned by the watch service.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WatchControlResponse {
    /// Simple acknowledgement without a payload.
    Ack {
        /// Human-readable acknowledgement for the caller.
        message: String,
    },
    /// Current service snapshot.
    Status {
        /// Latest service telemetry snapshot.
        snapshot: WatchDaemonState,
    },
    /// Reconcile outcome.
    Reconcile {
        /// Outcome of the reconcile pass.
        outcome: ReconcileOutcome,
        /// Triggering events (usually 0 for manual calls).
        triggering_events: usize,
    },
    /// Repair sync outcome.
    Sync {
        /// Summary of the completed sync pass.
        summary: SyncSummary,
    },
    /// Request failed.
    Error {
        /// Human-readable failure message.
        message: String,
    },
}

/// Send one control request to the live watch service for this repo.
///
/// The wire format is newline-delimited JSON in both directions: the client
/// writes a request object followed by `\n`, the server responds in kind.
/// This works portably across `interprocess::local_socket`'s backends (Unix
/// sockets and Windows named pipes) because local sockets do not expose a
/// portable half-close, so a length delimiter is required to frame messages.
pub fn request_watch_control(
    synrepo_dir: &Path,
    request: WatchControlRequest,
) -> Result<WatchControlResponse, WatchDaemonError> {
    let endpoint = resolve_control_endpoint(synrepo_dir);
    let io_err = |source| WatchDaemonError::Io {
        path: PathBuf::from(&endpoint),
        source,
    };

    let name = watch_control_socket_name(&endpoint).map_err(io_err)?;
    let stream = Stream::connect(name).map_err(io_err)?;
    write_control_request(&stream, &endpoint, &request)?;

    let mut reader = BufReader::new(&stream);
    let mut response_line = String::new();
    reader.read_line(&mut response_line).map_err(io_err)?;

    serde_json::from_str(response_line.trim_end_matches('\n')).map_err(|error| {
        WatchDaemonError::Control(format!("invalid watch control response: {error}"))
    })
}

/// Framing shared by the client and the in-process listener thread. The
/// server reads one line, parses it as JSON, and writes a newline-terminated
/// response.
pub(super) fn read_control_request(
    reader: &mut BufReader<&Stream>,
    endpoint: &str,
) -> Result<WatchControlRequest, WatchDaemonError> {
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|source| io_err(endpoint, source))?;
    serde_json::from_str(line.trim_end_matches('\n')).map_err(|error| {
        WatchDaemonError::Control(format!("invalid watch control request: {error}"))
    })
}

fn write_control_request(
    mut stream: &Stream,
    endpoint: &str,
    request: &WatchControlRequest,
) -> Result<(), WatchDaemonError> {
    let mut json = serde_json::to_vec(request).expect("WatchControlRequest serializes");
    json.push(b'\n');
    stream
        .write_all(&json)
        .map_err(|source| io_err(endpoint, source))
}

pub(super) fn write_control_response(
    mut stream: &Stream,
    endpoint: &str,
    response: &WatchControlResponse,
) -> Result<(), WatchDaemonError> {
    let mut json = serde_json::to_vec(response).expect("WatchControlResponse serializes");
    json.push(b'\n');
    stream
        .write_all(&json)
        .map_err(|source| io_err(endpoint, source))
}

fn io_err(endpoint: &str, source: std::io::Error) -> WatchDaemonError {
    WatchDaemonError::Io {
        path: PathBuf::from(endpoint),
        source,
    }
}

/// Probe whether the per-repo watch control endpoint is bound and accepting
/// connections.
///
/// `WatchServiceStatus::Running` only proves the daemon holds the lease and
/// has written its state file; the listener thread may not have bound the
/// control socket yet. A client calling [`request_watch_control`] in that
/// window gets ENOENT, which is indistinguishable from a dead daemon from
/// the client's point of view. Callers that care about readiness (e.g.
/// [`crate::tui::actions::stop_watch`]) should gate on this probe instead of
/// on status alone.
pub fn control_endpoint_reachable(synrepo_dir: &Path) -> bool {
    let endpoint = resolve_control_endpoint(synrepo_dir);
    let name = match watch_control_socket_name(&endpoint) {
        Ok(name) => name,
        Err(_) => return false,
    };
    Stream::connect(name).is_ok()
}

/// Prefer the bind-time endpoint persisted in `watch-daemon.json`; fall back
/// to recomputing only if the state file is missing or its `control_endpoint`
/// is empty (legacy state files written before the field was populated).
///
/// Why: `watch_control_endpoint` reads `$HOME` (and `$XDG_RUNTIME_DIR`,
/// `$USER`) via `user_socket_dir`. If the env changes between the daemon's
/// bind and a later client request — common in tests that mutate `$HOME`
/// concurrently with a watch service — recomputing produces a different
/// socket path and the request goes to the wrong (or non-existent) endpoint.
/// The state file holds the path the daemon actually bound, which is the
/// canonical answer.
fn resolve_control_endpoint(synrepo_dir: &Path) -> String {
    match load_watch_state(synrepo_dir) {
        Ok(state) if !state.control_endpoint.is_empty() => state.control_endpoint,
        _ => watch_control_endpoint(synrepo_dir),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use interprocess::local_socket::{ListenerNonblockingMode, ListenerOptions};

    #[test]
    fn endpoint_unreachable_when_no_listener_bound() {
        let tempdir = tempfile::tempdir().unwrap();
        let synrepo_dir = tempdir.path().join(".synrepo");
        std::fs::create_dir_all(&synrepo_dir).unwrap();

        assert!(!control_endpoint_reachable(&synrepo_dir));
    }

    #[test]
    fn endpoint_reachable_once_listener_is_bound() {
        let tempdir = tempfile::tempdir().unwrap();
        let synrepo_dir = tempdir.path().join(".synrepo");
        std::fs::create_dir_all(&synrepo_dir).unwrap();

        let endpoint = watch_control_endpoint(&synrepo_dir);
        #[cfg(unix)]
        let _ = std::fs::remove_file(&endpoint);
        let name = watch_control_socket_name(&endpoint).unwrap();
        let _listener = ListenerOptions::new()
            .name(name)
            .nonblocking(ListenerNonblockingMode::Accept)
            .create_sync()
            .unwrap();

        assert!(control_endpoint_reachable(&synrepo_dir));
    }
}
