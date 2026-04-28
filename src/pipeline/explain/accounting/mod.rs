//! Explain accounting: append-only per-call log plus an aggregates
//! snapshot, both under `.synrepo/state/`.
//!
//! Two files:
//!
//! - `.synrepo/state/explain-log.jsonl` — one JSON record per call, written
//!   via plain append. Crash-safe for our record sizes (small JSON lines
//!   well under a filesystem page).
//! - `.synrepo/state/explain-totals.json` — small aggregates blob.
//!   Rewritten on each update via [`crate::util::atomic_write::atomic_write`]
//!   (temp file + fsync + rename) so a crash never leaves it truncated.
//!
//! This module does not hold any long-lived state; [`record_event`] is
//! invoked synchronously from [`crate::pipeline::explain::telemetry::publish::publish`] after every
//! event is fanned out.

mod record;
mod storage;
#[cfg(test)]
mod tests;
mod types;

pub use storage::{load_totals, log_path, record_event, reset, totals_path};
pub use types::{ExplainCallRecord, ExplainTotals, ProviderTotals};
