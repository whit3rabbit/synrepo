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
    path::{Path, PathBuf},
    sync::mpsc,
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
        Ok(summary) => ReconcileOutcome::Completed(summary),
        Err(err) => ReconcileOutcome::Failed(err.to_string()),
    }
}

/// Persist a reconcile outcome to `.synrepo/state/reconcile-state.json`.
///
/// Silently ignores I/O errors; diagnostics reflect missing state as unknown
/// rather than blocking the caller on a write failure.
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

    if let Ok(json) = serde_json::to_string(&state) {
        let state_dir = synrepo_dir.join("state");
        let _ = std::fs::create_dir_all(&state_dir);
        let _ = std::fs::write(state_dir.join(RECONCILE_STATE_FILENAME), json);
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
mod tests {
    use super::*;
    use crate::store::compatibility::write_runtime_snapshot;
    use std::fs;
    use tempfile::tempdir;

    fn setup_test_repo(dir: &tempfile::TempDir) -> (PathBuf, Config, PathBuf) {
        let repo = dir.path().to_path_buf();
        fs::create_dir_all(repo.join("src")).unwrap();
        fs::write(repo.join("src/lib.rs"), "pub fn hello() {}\n").unwrap();
        let synrepo_dir = repo.join(".synrepo");
        // Compatibility snapshot is not required by run_reconcile_pass, but
        // writing one keeps tests that check diagnostics from warning about
        // missing state.
        fs::create_dir_all(synrepo_dir.join("state")).unwrap();
        write_runtime_snapshot(&synrepo_dir, &Config::default()).unwrap();
        (repo, Config::default(), synrepo_dir)
    }

    #[test]
    fn reconcile_pass_completes_on_valid_repo() {
        let dir = tempdir().unwrap();
        let (repo, config, synrepo_dir) = setup_test_repo(&dir);

        let outcome = run_reconcile_pass(&repo, &config, &synrepo_dir);
        assert!(
            matches!(outcome, ReconcileOutcome::Completed(_)),
            "expected Completed, got {}",
            outcome.as_str(),
        );

        if let ReconcileOutcome::Completed(ref summary) = outcome {
            assert!(summary.files_discovered >= 1, "must discover src/lib.rs");
            assert!(summary.symbols_extracted >= 1, "must extract hello()");
        }
    }

    #[test]
    fn reconcile_pass_returns_lock_conflict_when_lock_is_held() {
        let dir = tempdir().unwrap();
        let (repo, config, synrepo_dir) = setup_test_repo(&dir);

        // Hold the writer lock for the current process.
        let _lock = crate::pipeline::writer::acquire_writer_lock(&synrepo_dir).unwrap();

        // The reconcile pass must not block; it must report LockConflict.
        let outcome = run_reconcile_pass(&repo, &config, &synrepo_dir);
        assert!(
            matches!(outcome, ReconcileOutcome::LockConflict { .. }),
            "expected LockConflict, got {}",
            outcome.as_str(),
        );
    }

    #[test]
    fn reconcile_pass_corrects_stale_graph_state() {
        let dir = tempdir().unwrap();
        let (repo, config, synrepo_dir) = setup_test_repo(&dir);

        // First reconcile populates the graph.
        let first = run_reconcile_pass(&repo, &config, &synrepo_dir);
        assert!(matches!(first, ReconcileOutcome::Completed(_)));

        // Add a new file without running another reconcile (simulate watcher
        // miss during a branch switch or background build).
        fs::write(repo.join("src/new.rs"), "pub fn new_fn() {}\n").unwrap();

        // The reconcile backstop must pick up the new file.
        let second = run_reconcile_pass(&repo, &config, &synrepo_dir);
        if let ReconcileOutcome::Completed(summary) = second {
            assert!(
                summary.files_discovered >= 2,
                "new file must be discovered on reconcile fallback"
            );
        } else {
            panic!("expected Completed after adding new file");
        }
    }

    #[test]
    fn persist_and_load_reconcile_state_roundtrip() {
        let dir = tempdir().unwrap();
        let synrepo_dir = dir.path().join(".synrepo");

        let summary = CompileSummary {
            files_discovered: 5,
            symbols_extracted: 12,
            ..CompileSummary::default()
        };
        let outcome = ReconcileOutcome::Completed(summary);
        persist_reconcile_state(&synrepo_dir, &outcome, 3);

        let state = load_reconcile_state(&synrepo_dir).unwrap();
        assert_eq!(state.last_outcome, "completed");
        assert_eq!(state.files_discovered, Some(5));
        assert_eq!(state.symbols_extracted, Some(12));
        assert_eq!(state.triggering_events, 3);
        assert!(state.last_error.is_none());
    }

    #[test]
    fn persist_reconcile_state_records_failure_message() {
        let dir = tempdir().unwrap();
        let synrepo_dir = dir.path().join(".synrepo");

        let outcome = ReconcileOutcome::Failed("disk full".to_string());
        persist_reconcile_state(&synrepo_dir, &outcome, 0);

        let state = load_reconcile_state(&synrepo_dir).unwrap();
        assert_eq!(state.last_outcome, "failed");
        assert_eq!(state.last_error.as_deref(), Some("disk full"));
        assert!(state.files_discovered.is_none());
    }

    #[test]
    fn persist_reconcile_state_records_lock_conflict() {
        let dir = tempdir().unwrap();
        let synrepo_dir = dir.path().join(".synrepo");

        let outcome = ReconcileOutcome::LockConflict { holder_pid: 42 };
        persist_reconcile_state(&synrepo_dir, &outcome, 1);

        let state = load_reconcile_state(&synrepo_dir).unwrap();
        assert_eq!(state.last_outcome, "lock-conflict");
        assert!(state.last_error.is_none());
        assert_eq!(state.triggering_events, 1);
    }
}
