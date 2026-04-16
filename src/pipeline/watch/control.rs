use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::lease::{watch_socket_path, WatchDaemonError, WatchDaemonState};
use super::reconcile::ReconcileOutcome;

/// Control message sent over the per-repo watch socket.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum WatchControlRequest {
    /// Return the current service snapshot.
    Status,
    /// Stop the service and release the lease.
    Stop,
    /// Run one reconcile pass immediately.
    ReconcileNow,
}

/// Control response returned by the watch service.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
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
    /// Request failed.
    Error {
        /// Human-readable failure message.
        message: String,
    },
}

/// Send one control request to the live watch service for this repo.
pub fn request_watch_control(
    synrepo_dir: &Path,
    request: WatchControlRequest,
) -> Result<WatchControlResponse, WatchDaemonError> {
    #[cfg(unix)]
    {
        use std::io::Read as _;
        use std::os::unix::net::UnixStream;

        let socket_path = watch_socket_path(synrepo_dir);
        let mut stream =
            UnixStream::connect(&socket_path).map_err(|source| WatchDaemonError::Io {
                path: socket_path.clone(),
                source,
            })?;
        write_control_request(&mut stream, &request)?;
        stream
            .shutdown(std::net::Shutdown::Write)
            .map_err(|source| WatchDaemonError::Io {
                path: socket_path.clone(),
                source,
            })?;

        let mut response_json = String::new();
        stream
            .read_to_string(&mut response_json)
            .map_err(|source| WatchDaemonError::Io {
                path: socket_path.clone(),
                source,
            })?;

        serde_json::from_str(&response_json).map_err(|error| {
            WatchDaemonError::Control(format!("invalid watch control response: {error}"))
        })
    }
    #[cfg(not(unix))]
    {
        let _ = (synrepo_dir, request);
        Err(WatchDaemonError::Control(
            "watch daemon control is only supported on unix-like platforms".to_string(),
        ))
    }
}

#[cfg(unix)]
pub(super) fn read_control_request(
    stream: &mut std::os::unix::net::UnixStream,
) -> Result<WatchControlRequest, WatchDaemonError> {
    use std::io::Read as _;

    let mut json = String::new();
    stream
        .read_to_string(&mut json)
        .map_err(|source| WatchDaemonError::Io {
            path: PathBuf::from("<watch-control-stream>"),
            source,
        })?;
    serde_json::from_str(&json).map_err(|error| {
        WatchDaemonError::Control(format!("invalid watch control request: {error}"))
    })
}

#[cfg(unix)]
pub(super) fn write_control_request(
    stream: &mut std::os::unix::net::UnixStream,
    request: &WatchControlRequest,
) -> Result<(), WatchDaemonError> {
    use std::io::Write as _;

    let json = serde_json::to_vec(request).expect("WatchControlRequest serializes");
    stream
        .write_all(&json)
        .map_err(|source| WatchDaemonError::Io {
            path: PathBuf::from("<watch-control-stream>"),
            source,
        })
}

#[cfg(unix)]
pub(super) fn write_control_response(
    stream: &mut std::os::unix::net::UnixStream,
    response: &WatchControlResponse,
) -> Result<(), WatchDaemonError> {
    use std::io::Write as _;

    let json = serde_json::to_vec(response).expect("WatchControlResponse serializes");
    stream
        .write_all(&json)
        .map_err(|source| WatchDaemonError::Io {
            path: PathBuf::from("<watch-control-stream>"),
            source,
        })
}
