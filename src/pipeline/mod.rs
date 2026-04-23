//! Pipeline module: structural compile, watch/reconcile, and explain.
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
//! - `git` — deterministic repository-state inspection shared by structural
//!   provenance now and future git-intelligence work later.
//! - `diagnostics` — operational diagnostics surface for reconcile health,
//!   writer ownership, and stale runtime state.
//! - `maintenance` — maintenance hooks that consume the storage-compatibility
//!   contract for cleanup and compaction behavior.
//! - `compact` — compaction operations for overlay, state, and index stores.
//! - `git_intelligence` — the deterministic entry point for future
//!   history-derived routing and change-risk enrichment.
//! - `explain` — LLM-driven overlay pipeline (phase 4+).

pub mod compact;
pub mod context_metrics;
/// Operational diagnostics surface for reconcile health, writer ownership,
/// and store compatibility.
pub mod diagnostics;
pub mod explain;
pub mod export;
pub mod git;
pub mod git_intelligence;
pub mod maintenance;
pub mod recent_activity;
pub mod repair;
pub mod structural;
pub mod watch;
pub mod writer;
