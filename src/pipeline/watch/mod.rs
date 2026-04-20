//! Watch-triggered reconcile loop and optional daemon-assisted watch service.
//!
//! The watch runtime remains a trigger-and-coalesce layer over
//! `run_structural_compile`, not a second source of graph facts. Startup and
//! on-demand reconciles still flow through `run_reconcile_pass`.

mod control;
mod filter;
pub(crate) mod lease;
pub(crate) mod reconcile;
mod service;
mod status;

pub use control::{request_watch_control, WatchControlRequest, WatchControlResponse};
pub use lease::{
    watch_daemon_state_path, watch_socket_path, WatchDaemonError, WatchDaemonState,
    WatchServiceMode,
};
#[doc(hidden)]
pub use lease::{hold_watch_flock_with_state, TestWatchFlockHolder};
pub use reconcile::{
    emit_cochange_edges_pass, emit_symbol_revisions_pass, load_reconcile_state,
    persist_reconcile_state, reconcile_state_path, run_reconcile_pass, ReconcileOutcome,
    ReconcileState, ReconcileStateError,
};
pub use service::{run_watch_loop, run_watch_service, WatchConfig, WatchEvent};
pub use status::{
    cleanup_stale_watch_artifacts, load_watch_state, watch_service_status, StateLoadError,
    WatchServiceStatus,
};

#[cfg(test)]
mod tests;
