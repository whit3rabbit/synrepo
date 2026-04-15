use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize};

use crate::{config::Config, store::sqlite::SqliteGraphStore};

use super::super::{
    structural::{run_structural_compile, CompileSummary},
    writer::{acquire_writer_lock, now_rfc3339, LockError},
};

const RECONCILE_STATE_FILENAME: &str = "reconcile-state.json";
static NEXT_RECONCILE_STATE_TMP_ID: AtomicU64 = AtomicU64::new(0);

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
pub fn run_reconcile_pass(
    repo_root: &Path,
    config: &Config,
    synrepo_dir: &Path,
) -> ReconcileOutcome {
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
            if let Err(err) = emit_cochange_edges_pass(repo_root, config, &mut graph) {
                tracing::warn!(error = %err, "co-change edge emission failed; continuing");
            }
            if let Err(err) = crate::substrate::build_index(config, repo_root) {
                return ReconcileOutcome::Failed(format!("index rebuild failed: {err}"));
            }
            prune_overlay_orphans(synrepo_dir, &graph);
            ReconcileOutcome::Completed(summary)
        }
        Err(err) => ReconcileOutcome::Failed(err.to_string()),
    }
}

/// Re-emit all CoChangesWith edges from the current git history.
///
/// Full re-emit strategy: delete all existing CoChangesWith edges, then
/// re-derive from the current `GitHistoryInsights`. Runs in its own
/// transaction so a failure does not affect the structural compile.
pub fn emit_cochange_edges_pass(
    repo_root: &Path,
    config: &Config,
    graph: &mut dyn crate::structure::graph::GraphStore,
) -> crate::Result<()> {
    use crate::pipeline::git::GitIntelligenceContext;
    use crate::pipeline::git_intelligence::{analyze_recent_history, emit_cochange_edges};
    use crate::structure::graph::EdgeKind;
    use std::collections::HashMap;

    let context = GitIntelligenceContext::inspect(repo_root, config);
    let max_commits = config.git_commit_depth as usize;
    let insights = analyze_recent_history(&context, max_commits, 100)?;
    let revision = insights.history.status.source_revision.clone();

    let file_paths = graph.all_file_paths()?;
    let file_index: HashMap<String, crate::core::ids::FileNodeId> =
        file_paths.into_iter().collect();

    graph.begin()?;
    if let Err(err) = (|| -> crate::Result<()> {
        graph.delete_edges_by_kind(EdgeKind::CoChangesWith)?;
        emit_cochange_edges(graph, &insights, &file_index, &revision)?;
        Ok(())
    })() {
        let _ = graph.rollback();
        return Err(err);
    }
    graph.commit()?;
    Ok(())
}

fn prune_overlay_orphans(synrepo_dir: &Path, graph: &SqliteGraphStore) {
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
            tracing::warn!(error = %err, "overlay: open failed, skipping orphan prune");
            return;
        }
    };
    match overlay.prune_orphans(&live) {
        Ok(n) if n > 0 => {
            tracing::debug!(
                pruned = n,
                "overlay: pruned orphaned rows (commentary + cross-links)"
            )
        }
        Ok(_) => {}
        Err(err) => {
            tracing::warn!(error = %err, "overlay: prune failed");
        }
    }
}

/// Persist a reconcile outcome to `.synrepo/state/reconcile-state.json`.
pub fn persist_reconcile_state(
    synrepo_dir: &Path,
    outcome: &ReconcileOutcome,
    triggering_events: usize,
) {
    let (last_error, files_discovered, symbols_extracted) = match outcome {
        ReconcileOutcome::Completed(summary) => (
            None,
            Some(summary.files_discovered),
            Some(summary.symbols_extracted),
        ),
        ReconcileOutcome::Failed(message) => (Some(message.clone()), None, None),
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
        Ok(json) => json,
        Err(error) => {
            tracing::warn!(error = %error, "failed to serialize reconcile state");
            return;
        }
    };

    let state_dir = synrepo_dir.join("state");
    if let Err(error) = fs::create_dir_all(&state_dir) {
        tracing::warn!(
            path = ?state_dir,
            error = %error,
            "failed to create state dir for reconcile state"
        );
        return;
    }

    let final_path = state_dir.join(RECONCILE_STATE_FILENAME);
    let tmp_path = reconcile_state_tmp_path(&state_dir);
    if let Err(error) =
        fs::write(&tmp_path, json.as_bytes()).and_then(|_| fs::rename(&tmp_path, &final_path))
    {
        tracing::warn!(path = ?final_path, error = %error, "failed to persist reconcile state");
        let _ = fs::remove_file(&tmp_path);
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
