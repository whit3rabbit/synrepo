// Blocking watch-control socket bridge shared by foreground and daemon mode.

#[cfg(unix)]
use std::fs;
use std::{
    io::BufReader,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::Duration,
};

use interprocess::local_socket::{
    traits::{Listener as _, Stream as _},
    ListenerNonblockingMode, ListenerOptions,
};

use super::{
    control::{
        read_control_request, write_control_response, WatchControlRequest, WatchControlResponse,
    },
    lease::{watch_control_socket_name, WatchStateHandle},
    loop_message::LoopMessage,
};
use crate::pipeline::repair::SyncOptions;

/// Bind the watch control socket using `endpoint` as the canonical path.
///
/// `endpoint` MUST be the same string the daemon persisted in
/// `WatchDaemonState::control_endpoint` at lease acquisition; recomputing it
/// here would re-read `$HOME` (via `user_socket_dir`) and risk diverging from
/// the path clients read out of the state file. The caller — `run_watch_service`
/// — owns that canonical value and passes it through.
pub(super) fn spawn_control_listener(
    endpoint: String,
    state_handle: WatchStateHandle,
    tx: mpsc::Sender<LoopMessage>,
    stop_flag: Arc<AtomicBool>,
    auto_sync_enabled: Arc<AtomicBool>,
    auto_sync_blocked: Arc<AtomicBool>,
    sync_timeout_seconds: u32,
) -> crate::Result<thread::JoinHandle<()>> {
    // Unix sockets are filesystem paths. A prior dead daemon may have left a
    // stale socket file behind, which makes `bind()` fail with EADDRINUSE.
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
                    // Prevent abandoned connections from blocking a handler
                    // thread forever while waiting for a newline-framed request.
                    set_stream_read_timeout(&stream, Duration::from_secs(5));

                    let state_handle = state_handle.clone();
                    let tx = tx.clone();
                    let stop_flag = stop_flag.clone();
                    let auto_sync_enabled = auto_sync_enabled.clone();
                    let auto_sync_blocked = auto_sync_blocked.clone();
                    let endpoint = endpoint.clone();

                    let sync_timeout = Duration::from_secs(sync_timeout_seconds as u64);
                    thread::spawn(move || {
                        let mut reader = BufReader::new(&stream);
                        let request_result = read_control_request(&mut reader, &endpoint);
                        let response = match request_result {
                            Ok(WatchControlRequest::Status) => WatchControlResponse::Status {
                                snapshot: state_handle.snapshot(),
                            },
                            Ok(WatchControlRequest::Stop) => bridge_stop_request(&tx, &stop_flag),
                            Ok(WatchControlRequest::ReconcileNow { fast }) => {
                                bridge_reconcile_request(&tx, fast)
                            }
                            Ok(WatchControlRequest::SuppressPaths { paths, ttl_ms }) => {
                                bridge_suppress_paths_request(&tx, paths, ttl_ms)
                            }
                            Ok(WatchControlRequest::SyncNow { options }) => {
                                bridge_sync_request(&tx, options, sync_timeout)
                            }
                            Ok(WatchControlRequest::EmbeddingsBuildNow) => {
                                bridge_embeddings_build_request(&tx, sync_timeout)
                            }
                            Ok(WatchControlRequest::SetAutoSync { enabled }) => {
                                auto_sync_enabled.store(enabled, Ordering::Relaxed);
                                if enabled {
                                    auto_sync_blocked.store(false, Ordering::Relaxed);
                                }
                                state_handle.note_auto_sync_enabled(enabled);
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
                    });
                }
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

fn set_stream_read_timeout(stream: &interprocess::local_socket::Stream, timeout: Duration) {
    if let Err(error) = stream.set_recv_timeout(Some(timeout)) {
        tracing::warn!(error = %error, "failed to set watch control stream read timeout");
    }
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

fn bridge_reconcile_request(tx: &mpsc::Sender<LoopMessage>, fast: bool) -> WatchControlResponse {
    let (respond_to, recv_from_loop) = mpsc::channel();
    if tx
        .send(LoopMessage::ReconcileNow { respond_to, fast })
        .is_err()
    {
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

fn bridge_suppress_paths_request(
    tx: &mpsc::Sender<LoopMessage>,
    paths: Vec<std::path::PathBuf>,
    ttl_ms: u64,
) -> WatchControlResponse {
    let (respond_to, recv_from_loop) = mpsc::channel();
    if tx
        .send(LoopMessage::SuppressPaths {
            respond_to,
            paths,
            ttl: Duration::from_millis(ttl_ms),
        })
        .is_err()
    {
        return WatchControlResponse::Error {
            message: "watch loop is no longer accepting control messages".to_string(),
        };
    }

    recv_from_loop
        .recv_timeout(Duration::from_secs(5))
        .unwrap_or_else(|_| WatchControlResponse::Error {
            message: "watch loop did not answer the control request in time".to_string(),
        })
}

fn bridge_sync_request(
    tx: &mpsc::Sender<LoopMessage>,
    options: SyncOptions,
    timeout: Duration,
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

    // Sync may invoke LLM-backed commentary refresh; the timeout is sourced
    // from `Config::watch_sync_timeout_seconds` so big-repo refreshes can be
    // tuned without recompiling. A wedged loop still surfaces eventually.
    recv_from_loop
        .recv_timeout(timeout)
        .unwrap_or_else(|_| WatchControlResponse::Error {
            message: "watch loop did not answer the sync request in time".to_string(),
        })
}

fn bridge_embeddings_build_request(
    tx: &mpsc::Sender<LoopMessage>,
    timeout: Duration,
) -> WatchControlResponse {
    let (respond_to, recv_from_loop) = mpsc::channel();
    if tx
        .send(LoopMessage::EmbeddingsBuildNow { respond_to })
        .is_err()
    {
        return WatchControlResponse::Error {
            message: "watch loop is no longer accepting control messages".to_string(),
        };
    }

    recv_from_loop
        .recv_timeout(timeout)
        .unwrap_or_else(|_| WatchControlResponse::Error {
            message: "watch loop did not answer the embeddings build request in time".to_string(),
        })
}
