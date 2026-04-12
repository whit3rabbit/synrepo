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
//! | `StaleRationale` | not yet implemented (reported as Unsupported) |
//! | `OverlayEntries` | not yet implemented (reported as Unsupported) |
//! | `ExportViews` | not yet implemented (reported as Unsupported) |
//!
//! ## Resolution log
//!
//! Each mutating `sync` run appends a JSONL record to
//! `.synrepo/state/repair-log.jsonl`.

mod declared_links;
mod log;
mod report;
mod sync;
mod types;

#[cfg(test)]
mod tests;

pub use log::{append_resolution_log, repair_log_path};
pub use report::build_repair_report;
pub use sync::execute_sync;
pub use types::{
    DriftClass, RepairAction, RepairFinding, RepairReport, RepairSurface, ResolutionLogEntry,
    Severity, SyncOutcome, SyncSummary,
};
