use crate::pipeline::{
    repair::{SyncProgress, SyncSummary},
    watch::reconcile::ReconcileOutcome,
};
use crate::substrate::embedding::{EmbeddingBuildEvent, EmbeddingBuildSummary};

/// Why a reconcile pass chose full rebuild instead of scoped incremental work.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReconcileStartReason {
    /// The debounced watch batch exceeded the incremental touched-path cap.
    WatchPathOverflow,
}

/// Why a sync pass is running inside the watch service.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncTrigger {
    /// CLI sent `SyncNow` over the control socket, or the TUI pressed `S`.
    Manual,
    /// The reconcile loop opted into auto-sync for cheap surfaces.
    AutoPostReconcile,
}

/// Why an embedding index job is running inside the watch service.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddingTrigger {
    /// CLI or TUI asked watch to build vectors explicitly.
    Manual,
    /// Watch refreshed an existing stale vector index after the repo went quiet.
    AutoRefresh,
}

/// Event emitted by the watch service for each reconcile attempt and error.
///
/// Used by the live-mode dashboard to stream activity into the log pane.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum WatchEvent {
    /// Emitted immediately before `run_reconcile_pass` runs.
    ReconcileStarted {
        /// RFC 3339 UTC timestamp when the pass started.
        at: String,
        /// Number of debounced filesystem events that triggered this pass.
        triggering_events: usize,
        /// True when this pass is a full reconcile rather than scoped to touched paths.
        full: bool,
        /// Optional reason a full reconcile was forced.
        reason: Option<ReconcileStartReason>,
    },
    /// Emitted after a reconcile pass completes with its outcome.
    ReconcileFinished {
        /// RFC 3339 UTC timestamp when the pass finished.
        at: String,
        /// Final outcome from `run_reconcile_pass`.
        outcome: ReconcileOutcome,
        /// Number of debounced filesystem events that triggered this pass.
        triggering_events: usize,
    },
    /// Emitted before a repair sync pass runs inside the watch service.
    SyncStarted {
        /// RFC 3339 UTC timestamp when the pass started.
        at: String,
        /// Whether this is an operator-requested or auto-triggered sync.
        trigger: SyncTrigger,
    },
    /// Emitted for each surface boundary and commentary sub-event during sync.
    SyncProgress {
        /// RFC 3339 UTC timestamp when the progress event was emitted.
        at: String,
        /// The structured progress payload.
        progress: SyncProgress,
    },
    /// Emitted when a sync pass finishes, with the resulting summary.
    SyncFinished {
        /// RFC 3339 UTC timestamp when the pass finished.
        at: String,
        /// Why the sync ran.
        trigger: SyncTrigger,
        /// Completed summary.
        summary: SyncSummary,
    },
    /// Emitted before an embedding build or refresh starts.
    EmbeddingStarted {
        /// RFC 3339 UTC timestamp when the job started.
        at: String,
        /// Whether this is explicit or automatic work.
        trigger: EmbeddingTrigger,
    },
    /// Emitted for embedding build progress.
    EmbeddingProgress {
        /// RFC 3339 UTC timestamp when the progress event was emitted.
        at: String,
        /// Whether this is explicit or automatic work.
        trigger: EmbeddingTrigger,
        /// Structured embedding progress payload.
        progress: EmbeddingBuildEvent,
    },
    /// Emitted when an embedding build or refresh finishes.
    EmbeddingFinished {
        /// RFC 3339 UTC timestamp when the job finished.
        at: String,
        /// Whether this is explicit or automatic work.
        trigger: EmbeddingTrigger,
        /// Completed summary when successful.
        summary: Option<EmbeddingBuildSummary>,
        /// Human-readable error when the job failed.
        error: Option<String>,
    },
    /// Emitted for watcher-level errors.
    Error {
        /// RFC 3339 UTC timestamp when the error was observed.
        at: String,
        /// Human-readable error description.
        message: String,
    },
}
