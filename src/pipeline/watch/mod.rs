//! Watch-triggered reconcile loop and optional daemon-assisted watch service.
//!
//! The watch runtime remains a trigger-and-coalesce layer over
//! `run_structural_compile`, not a second source of graph facts. Startup and
//! on-demand reconciles still flow through `run_reconcile_pass`.

mod control;
mod control_bridge;
mod events;
mod filter;
pub(crate) mod lease;
mod pending;
mod post_compile;
pub(crate) mod reconcile;
mod reconcile_state;
mod service;
mod status;
mod suppression;
mod sync;

pub use control::{
    control_endpoint_reachable, request_watch_control, WatchControlRequest, WatchControlResponse,
};
pub use events::{ReconcileStartReason, SyncTrigger, WatchEvent};
#[doc(hidden)]
pub use lease::{hold_watch_flock_with_state, TestWatchFlockHolder};
pub use lease::{
    watch_daemon_state_path, watch_socket_path, WatchDaemonError, WatchDaemonState,
    WatchServiceMode,
};
pub(crate) use post_compile::{finish_runtime_surfaces, RepoIndexStrategy};
pub use reconcile::{
    emit_cochange_edges_pass, emit_symbol_revisions_pass, run_reconcile_attempt,
    run_reconcile_pass, ReconcileAttempt, ReconcileOutcome,
};
pub use reconcile_state::{
    load_reconcile_state, persist_reconcile_attempt_state, persist_reconcile_state,
    reconcile_state_path, ReconcileState, ReconcileStateError,
};
pub use service::{run_watch_loop, run_watch_service, WatchConfig};
pub use status::{
    cleanup_stale_watch_artifacts, load_watch_state, watch_service_status, StateLoadError,
    WatchServiceStatus,
};

#[cfg(test)]
mod tests;
