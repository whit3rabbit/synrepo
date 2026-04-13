//! Watch-triggered reconcile loop and optional daemon-assisted watch service.
//!
//! The watch runtime remains a trigger-and-coalesce layer over
//! `run_structural_compile`, not a second source of graph facts. Startup and
//! on-demand reconciles still flow through `run_reconcile_pass`.

mod control;
mod lease;
mod reconcile;
mod service;

pub use control::{request_watch_control, WatchControlRequest, WatchControlResponse};
pub use lease::{
    cleanup_stale_watch_artifacts, load_watch_state, watch_daemon_state_path, watch_service_status,
    watch_socket_path, WatchDaemonError, WatchDaemonState, WatchServiceMode, WatchServiceStatus,
};
pub use reconcile::{
    load_reconcile_state, persist_reconcile_state, reconcile_state_path, run_reconcile_pass,
    ReconcileOutcome, ReconcileState,
};
pub use service::{run_watch_loop, run_watch_service, WatchConfig};

#[cfg(test)]
mod tests;
