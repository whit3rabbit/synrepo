use std::{
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
};

use interprocess::local_socket::{traits::Stream as _, Stream};
use serde::{Deserialize, Serialize};

use super::lease::{
    watch_control_endpoint, watch_control_socket_name, WatchDaemonError, WatchDaemonState,
};
use super::reconcile::ReconcileOutcome;

/// Control message sent over the per-repo watch control endpoint.
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
    let endpoint = watch_control_endpoint(synrepo_dir);
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
