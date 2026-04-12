//! Watch-triggered reconcile loop for keeping `.synrepo/` current under
//! normal repository churn.
//!
//! ## Architecture
//!
//! The watch loop wraps the structural compile path as a trigger-and-coalesce
//! layer, not a separate source of graph facts. The watcher is a latency
//! optimization; the reconcile pass is the correctness backstop.
//!
//! ## Reconcile backstop
//!
//! A startup reconcile runs before the watch loop begins, correcting any
//! state that changed while no watcher was active. Subsequent reconciles are
//! triggered by debounced filesystem events. Both paths run the same
//! `run_structural_compile` producer path.
//!
//! ## Event coalescing
//!
//! `notify-debouncer-full` coalesces filesystem events within the configured
//! `debounce_timeout` window. Each settled burst triggers exactly one
//! structural compile cycle regardless of how many raw events it contained,
//! bounding the compile rate during builds or refactors.
//!
//! ## Sequencing
//!
//! This module depends on `structural::run_structural_compile` as the
//! producer path it supervises. Watch and reconcile behavior is downstream of
//! the structural pipeline and reuses its deterministic compile path rather
//! than defining an independent graph producer. Git intelligence, overlay
//! refresh, and full daemon lifecycle are intentionally out of scope here.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::Duration,
};

use notify_debouncer_full::{
    new_debouncer,
    notify::{RecursiveMode, Watcher},
    DebounceEventResult,
};
use serde::{Deserialize, Serialize};

use crate::{config::Config, store::sqlite::SqliteGraphStore};

use super::{
    structural::{run_structural_compile, CompileSummary},
    writer::{acquire_writer_lock, now_rfc3339, LockError},
};

const RECONCILE_STATE_FILENAME: &str = "reconcile-state.json";
static NEXT_RECONCILE_STATE_TMP_ID: AtomicU64 = AtomicU64::new(0);

/// Configuration for the watch and reconcile loop.
#[derive(Clone, Debug)]
pub struct WatchConfig {
    /// How long after the last filesystem event to wait before triggering a
    /// reconcile pass. Shorter values reduce latency; longer values reduce
    /// CPU churn during high-frequency edits or build output.
    pub debounce_timeout: Duration,
    /// Upper bound on events logged per reconcile cycle. Events beyond this
    /// count are still reconciled, but the recorded count is capped to prevent
    /// unbounded growth in diagnostic records.
    pub max_events_per_cycle: usize,
}

impl Default for WatchConfig {
    fn default() -> Self {
        Self {
            debounce_timeout: Duration::from_millis(500),
            max_events_per_cycle: 1000,
        }
    }
}

/// Outcome of a single reconcile pass.
#[derive(Clone, Debug)]
pub enum ReconcileOutcome {
    /// The reconcile completed successfully.
    Completed(CompileSummary),
    /// Another live process held the writer lock; reconcile was skipped.
    LockConflict {
        /// PID of the process holding the lock at reconcile time.
        holder_pid: u32,
    },
    /// The reconcile failed due to an I/O or compile error.
    Failed(String),
}

impl ReconcileOutcome {
    /// Stable string identifier for this outcome variant.
    pub fn as_str(&self) -> &'static str {
        match self {
            ReconcileOutcome::Completed(_) => "completed",
            ReconcileOutcome::LockConflict { .. } => "lock-conflict",
            ReconcileOutcome::Failed(_) => "failed",
        }
    }
}

/// Persisted reconcile state written to `.synrepo/state/reconcile-state.json`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReconcileState {
    /// RFC 3339 UTC timestamp of the last reconcile attempt.
    pub last_reconcile_at: String,
    /// Outcome string: "completed", "lock-conflict", or "failed".
    pub last_outcome: String,
    /// Error message when the last reconcile failed, otherwise absent.
    pub last_error: Option<String>,
    /// Number of filesystem events that triggered this pass (0 = startup).
    pub triggering_events: usize,
    /// File count from the last completed compile; absent if not completed.
    pub files_discovered: Option<usize>,
    /// Symbol count from the last completed compile; absent if not completed.
    pub symbols_extracted: Option<usize>,
}

/// Run one full reconcile pass against the structural compile path.
///
/// Acquires the writer lock, opens the graph store, and runs
/// `run_structural_compile`. Returns `ReconcileOutcome::LockConflict`
/// immediately if another live process holds the lock, without blocking.
///
/// This function is the core correctness backstop: it can be called at
/// startup, on filesystem events, or on a schedule to restore graph state
/// after watcher misses.
pub fn run_reconcile_pass(
    repo_root: &Path,
    config: &Config,
    synrepo_dir: &Path,
) -> ReconcileOutcome {
    // Acquire the writer lock. Return LockConflict rather than blocking so
    // the watcher loop stays responsive when a concurrent writer is active.
    let _lock = match acquire_writer_lock(synrepo_dir) {
        Ok(lock) => lock,
        Err(LockError::HeldByOther { pid, .. }) => {
            return ReconcileOutcome::LockConflict { holder_pid: pid };
        }
        Err(err) => return ReconcileOutcome::Failed(err.to_string()),
    };

    let graph_dir = synrepo_dir.join("graph");
    let mut graph = match SqliteGraphStore::open(&graph_dir) {
        Ok(g) => g,
        Err(err) => return ReconcileOutcome::Failed(err.to_string()),
    };

    match run_structural_compile(repo_root, config, &mut graph) {
        Ok(summary) => {
            if let Err(err) = crate::substrate::build_index(config, repo_root) {
                return ReconcileOutcome::Failed(format!("index rebuild failed: {err}"));
            }
            prune_commentary_orphans(synrepo_dir, &graph);
            ReconcileOutcome::Completed(summary)
        }
        Err(err) => ReconcileOutcome::Failed(err.to_string()),
    }
}

/// After a successful structural compile, drop commentary entries for nodes
/// that no longer exist in the graph. The overlay store is opened only when
/// `overlay.db` already exists so reconcile never materializes the overlay
/// on its own (that stays lazy, created by the MCP server or first write).
fn prune_commentary_orphans(synrepo_dir: &Path, graph: &SqliteGraphStore) {
    use crate::core::ids::NodeId;
    use crate::overlay::OverlayStore;
    use crate::store::overlay::SqliteOverlayStore;
    use crate::structure::graph::GraphStore;

    let overlay_dir = synrepo_dir.join("overlay");
    let overlay_db = SqliteOverlayStore::db_path(&overlay_dir);
    if !overlay_db.exists() {
        return;
    }

    let mut live: Vec<NodeId> = Vec::new();
    if let Ok(files) = graph.all_file_paths() {
        live.extend(files.into_iter().map(|(_, id)| NodeId::File(id)));
    }
    if let Ok(concepts) = graph.all_concept_paths() {
        live.extend(concepts.into_iter().map(|(_, id)| NodeId::Concept(id)));
    }
    if let Ok(symbols) = graph.all_symbol_names() {
        live.extend(symbols.into_iter().map(|(id, _, _)| NodeId::Symbol(id)));
    }

    let mut overlay = match SqliteOverlayStore::open_existing(&overlay_dir) {
        Ok(o) => o,
        Err(err) => {
            tracing::warn!(error = %err, "commentary overlay: open failed, skipping orphan prune");
            return;
        }
    };
    match overlay.prune_orphans(&live) {
        Ok(n) if n > 0 => {
            tracing::debug!(pruned = n, "commentary overlay: pruned orphaned entries")
        }
        Ok(_) => {}
        Err(err) => {
            tracing::warn!(error = %err, "commentary overlay: prune failed");
        }
    }
}

/// Persist a reconcile outcome to `.synrepo/state/reconcile-state.json`.
///
/// Never blocks the caller. Writes atomically via a `.tmp` sibling file then
/// rename so readers never observe partial JSON. I/O failures are logged as
/// warnings; a missing state file is treated as "unknown" by diagnostics.
pub fn persist_reconcile_state(
    synrepo_dir: &Path,
    outcome: &ReconcileOutcome,
    triggering_events: usize,
) {
    let (last_error, files_discovered, symbols_extracted) = match outcome {
        ReconcileOutcome::Completed(s) => {
            (None, Some(s.files_discovered), Some(s.symbols_extracted))
        }
        ReconcileOutcome::Failed(msg) => (Some(msg.clone()), None, None),
        ReconcileOutcome::LockConflict { .. } => (None, None, None),
    };

    let state = ReconcileState {
        last_reconcile_at: now_rfc3339(),
        last_outcome: outcome.as_str().to_string(),
        last_error,
        triggering_events,
        files_discovered,
        symbols_extracted,
    };

    let json = match serde_json::to_string(&state) {
        Ok(j) => j,
        Err(e) => {
            tracing::warn!(error = %e, "failed to serialize reconcile state");
            return;
        }
    };

    let state_dir = synrepo_dir.join("state");
    if let Err(e) = std::fs::create_dir_all(&state_dir) {
        tracing::warn!(path = ?state_dir, error = %e, "failed to create state dir for reconcile state");
        return;
    }

    // Write to .tmp then rename: readers see either the old complete file or
    // the new complete file, never a partial write.
    let final_path = state_dir.join(RECONCILE_STATE_FILENAME);
    let tmp_path = reconcile_state_tmp_path(&state_dir);
    if let Err(e) =
        fs::write(&tmp_path, json.as_bytes()).and_then(|_| fs::rename(&tmp_path, &final_path))
    {
        tracing::warn!(path = ?final_path, error = %e, "failed to persist reconcile state");
        let _ = fs::remove_file(&tmp_path); // clean up orphaned .tmp
    }
}

/// Load the persisted reconcile state, if present and readable.
pub fn load_reconcile_state(synrepo_dir: &Path) -> Option<ReconcileState> {
    let path = reconcile_state_path(synrepo_dir);
    let text = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Canonical path of the reconcile state file.
pub fn reconcile_state_path(synrepo_dir: &Path) -> PathBuf {
    synrepo_dir.join("state").join(RECONCILE_STATE_FILENAME)
}

fn reconcile_state_tmp_path(state_dir: &Path) -> PathBuf {
    let id = NEXT_RECONCILE_STATE_TMP_ID.fetch_add(1, Ordering::Relaxed);
    state_dir.join(format!(
        "{RECONCILE_STATE_FILENAME}.tmp.{}.{}",
        std::process::id(),
        id
    ))
}

/// Run the watch loop, blocking the calling thread until a fatal error occurs.
///
/// Runs the startup reconcile backstop before entering the watch loop,
/// correcting any state that changed while unwatched. Each settled burst of
/// filesystem events then triggers one reconcile pass under the writer lock.
///
/// For CLI use, call this from the main thread after `bootstrap::bootstrap`
/// completes. The function returns only on fatal watcher setup failure; normal
/// operation is indefinitely blocking.
pub fn run_watch_loop(
    repo_root: &Path,
    config: &Config,
    watch_config: &WatchConfig,
    synrepo_dir: &Path,
) -> crate::Result<()> {
    let (tx, rx) = mpsc::channel::<DebounceEventResult>();

    let mut debouncer = new_debouncer(watch_config.debounce_timeout, None, move |result| {
        let _ = tx.send(result);
    })
    .map_err(|e| crate::Error::Other(anyhow::anyhow!("failed to create file watcher: {e}")))?;

    debouncer
        .watcher()
        .watch(repo_root, RecursiveMode::Recursive)
        .map_err(|e| {
            crate::Error::Other(anyhow::anyhow!(
                "failed to watch {}: {e}",
                repo_root.display()
            ))
        })?;

    // Startup reconcile backstop: correct any state that changed while
    // no watcher was active (e.g. branch switch, background build).
    let startup = run_reconcile_pass(repo_root, config, synrepo_dir);
    persist_reconcile_state(synrepo_dir, &startup, 0);
    tracing::info!(outcome = %startup.as_str(), "startup reconcile complete");

    loop {
        match rx.recv() {
            Ok(Ok(events)) => {
                // Coalescing: many raw events become one compile cycle.
                let event_count = events.len().min(watch_config.max_events_per_cycle);
                tracing::debug!(events = event_count, "coalesced events; running reconcile");
                let outcome = run_reconcile_pass(repo_root, config, synrepo_dir);
                persist_reconcile_state(synrepo_dir, &outcome, event_count);
                tracing::info!(
                    outcome = %outcome.as_str(),
                    events = event_count,
                    "reconcile pass complete"
                );
            }
            Ok(Err(errors)) => {
                for err in &errors {
                    tracing::warn!("watcher error: {err}");
                }
            }
            Err(_) => break, // Channel closed; watcher was dropped.
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests;
