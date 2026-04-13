//! Stable types for the repair finding and audit model.
//!
//! These types are serialized into repair reports and resolution log entries.
//! All string identifiers are stable across versions.

mod models;
mod stable;

#[cfg(test)]
mod tests;

pub use models::{
    RepairFinding, RepairReport, ResolutionLogEntry, SyncOptions, SyncOutcome, SyncSummary,
};
pub use stable::{DriftClass, RepairAction, RepairSurface, Severity};
