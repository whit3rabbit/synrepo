// Blocking watch-control socket bridge shared by foreground and daemon mode.

#[cfg(unix)]
use std::fs;
use std::{
    io::BufReader,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::Duration,
};

use interprocess::local_socket::{traits::Listener as _, ListenerNonblockingMode, ListenerOptions};

use super::{
    control::{
        read_control_request, write_control_response, WatchControlRequest, WatchControlResponse,
    },
    lease::{watch_control_endpoint, watch_control_socket_name, WatchStateHandle},
    service::LoopMessage,
};
use crate::pipeline::repair::SyncOptions;

pub(super) fn spawn_control_listener(
    synrepo_dir: &Path,
    state_handle: WatchStateHandle,
    tx: mpsc::Sender<LoopMessage>,
    stop_flag: Arc<AtomicBool>,
    auto_sync_enabled: Arc<AtomicBool>,
) -> crate::Result<thread::JoinHandle<()>> {
    let endpoint = watch_control_endpoint(synrepo_dir);

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
                    // Set a read timeout to prevent abandoned connections from
                    // leaking handler threads indefinitely.
                    set_stream_read_timeout(&stream, Duration::from_secs(5));

                    let state_handle = state_handle.clone();
                    let tx = tx.clone();
                    let stop_flag = stop_flag.clone();
                    let auto_sync_enabled = auto_sync_enabled.clone();
                    let endpoint = endpoint.clone();

                    thread::spawn(move || {
                        let mut reader = BufReader::new(&stream);
                        let request_result = read_control_request(&mut reader, &endpoint);
                        let response = match request_result {
                            Ok(WatchControlRequest::Status) => WatchControlResponse::Status {
                                snapshot: state_handle.snapshot(),
                            },
                            Ok(WatchControlRequest::Stop) => {
                                bridge_stop_request(&tx, &stop_flag)
                            }
                            Ok(WatchControlRequest::ReconcileNow { fast }) => {
                                bridge_reconcile_request(&tx, fast)
                            }
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
                        if let Err(error) =
                            write_control_response(&stream, &endpoint, &response)
                        {
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

/// Set `SO_RCVTIMEO` on the local socket stream so that a hanging client
/// (connects but never sends) does not leak the handler thread forever.
#[cfg(unix)]
fn set_stream_read_timeout(stream: &interprocess::local_socket::Stream, timeout: Duration) {
    use std::os::unix::io::AsRawFd;
    let fd = stream.as_raw_fd();
    let tv = libc::timeval {
        tv_sec: timeout.as_secs() as _,
        tv_usec: timeout.subsec_micros() as _,
    };
    let result = unsafe {
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_RCVTIMEO,
            &tv as *const _ as *const _,
            std::mem::size_of::<libc::timeval>() as _,
        )
    };
    if result != 0 {
        tracing::warn!("failed to set SO_RCVTIMEO on watch control stream");
    }
}

#[cfg(not(unix))]
fn set_stream_read_timeout(
    _stream: &interprocess::local_socket::Stream,
    _timeout: Duration,
) {
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
