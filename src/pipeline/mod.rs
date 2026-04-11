//! Pipeline module: structural compile, watch/reconcile, and synthesis.
//!
//! ## Module responsibilities
//!
//! - `structural` — the deterministic, LLM-free compile cycle that populates
//!   the graph from parser-observed and human-declared facts. This is the
//!   producer path that all other pipeline modules depend on.
//! - `watch` — watch-triggered reconcile loop and the startup reconcile
//!   backstop. Drives the structural compile path; does not produce graph
//!   facts independently.
//! - `writer` — single-writer lock contract for standalone CLI and future
//!   daemon-assisted operation.
//! - `diagnostics` — operational diagnostics surface for reconcile health,
//!   writer ownership, and stale runtime state.
//! - `maintenance` — maintenance hooks that consume the storage-compatibility
//!   contract for cleanup and compaction behavior.
//! - `synthesis` — LLM-driven overlay pipeline (phase 4+).

pub mod diagnostics;
pub mod maintenance;
pub mod structural;
pub mod synthesis;
pub mod watch;
pub mod writer;
