//! Repair finding and audit model for `synrepo check` and `synrepo sync`.
//!
//! Composes existing diagnostics, maintenance planning, and reconcile
//! primitives into a surface-oriented repair view. Each named surface gets
//! one finding with a drift class, severity, and recommended action.
//!
//! ## Surfaces checked
//!
//! | Surface | Producer |
//! |---------|---------|
//! | `WriterLock` | `diagnostics::WriterStatus` |
//! | `StoreMaintenance` | `maintenance::plan_maintenance` |
//! | `StructuralRefresh` | `diagnostics::ReconcileHealth` |
//! | `DeclaredLinks` | `Governs` edges + concept node stats |
//! | `StaleRationale` | `Governs` edges with drifted targets (Jaccard >= 0.5) |
//! | `CommentaryOverlayEntries` | live: absent / current / stale + `RefreshCommentary` |
//! | `ExportSurface` | export manifest freshness vs current reconcile epoch |
//!
//! ## Resolution log
//!
//! Each mutating `sync` run appends a JSONL record to
//! `.synrepo/state/repair-log.jsonl`.

mod commentary;
mod cross_link_verify;
mod cross_links;
mod declared_links;
mod log;
mod report;
mod sync;
mod types;

#[cfg(test)]
mod tests;

pub use commentary::{resolve_commentary_node, CommentaryNodeSnapshot};
pub use log::{
    append_resolution_log, read_repair_log_degraded_marker, repair_log_degraded_marker_path,
    repair_log_path, RepairLogDegraded,
};
pub use report::build_repair_report;
pub use report::surfaces::{scan_commentary_staleness, CommentaryScan};
pub use sync::{execute_sync, refresh_commentary, ActionContext};
pub use types::{
    DriftClass, RepairAction, RepairFinding, RepairReport, RepairSurface, ResolutionLogEntry,
    Severity, SyncOptions, SyncOutcome, SyncSummary,
};
