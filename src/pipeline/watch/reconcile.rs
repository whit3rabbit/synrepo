use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{config::Config, store::sqlite::SqliteGraphStore};

use super::super::{
    structural::{run_structural_compile, run_structural_compile_for_root_ids, CompileSummary},
    writer::{acquire_writer_lock, now_rfc3339, LockError},
};
use super::post_compile::{finish_runtime_surfaces, RepoIndexStrategy};

/// Outcome of a reconcile pass, written to reconcile-state.json.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
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

/// A reconcile outcome paired with the timestamp from before lock acquisition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReconcileAttempt {
    /// RFC 3339 UTC timestamp captured before attempting to acquire the writer lock.
    pub started_at: String,
    /// Outcome produced by the reconcile pass.
    pub outcome: ReconcileOutcome,
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

/// Run one full reconcile pass against the structural compile path.
///
/// Acquires the writer lock, opens the graph store, and runs
/// `run_structural_compile`. Returns `ReconcileOutcome::LockConflict`
/// immediately if another live process holds the lock, without blocking.
pub fn run_reconcile_pass(
    repo_root: &Path,
    config: &Config,
    synrepo_dir: &Path,
    fast: bool,
) -> ReconcileOutcome {
    run_reconcile_attempt(repo_root, config, synrepo_dir, fast).outcome
}

/// Run one full reconcile pass and return its attempt start timestamp.
pub fn run_reconcile_attempt(
    repo_root: &Path,
    config: &Config,
    synrepo_dir: &Path,
    fast: bool,
) -> ReconcileAttempt {
    run_reconcile_attempt_with_touched_paths(repo_root, config, synrepo_dir, None, fast)
}

#[cfg(test)]
pub(crate) fn run_reconcile_pass_with_touched_paths(
    repo_root: &Path,
    config: &Config,
    synrepo_dir: &Path,
    touched_paths: Option<&[PathBuf]>,
    fast: bool,
) -> ReconcileOutcome {
    run_reconcile_attempt_with_touched_paths(repo_root, config, synrepo_dir, touched_paths, fast)
        .outcome
}

pub(crate) fn run_reconcile_attempt_with_touched_paths(
    repo_root: &Path,
    config: &Config,
    synrepo_dir: &Path,
    touched_paths: Option<&[PathBuf]>,
    fast: bool,
) -> ReconcileAttempt {
    let started_at = now_rfc3339();
    let outcome = run_reconcile_pass_with_touched_paths_inner(
        repo_root,
        config,
        synrepo_dir,
        touched_paths,
        fast,
    );
    ReconcileAttempt {
        started_at,
        outcome,
    }
}

fn run_reconcile_pass_with_touched_paths_inner(
    repo_root: &Path,
    config: &Config,
    synrepo_dir: &Path,
    touched_paths: Option<&[PathBuf]>,
    fast: bool,
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

    let active_root_ids =
        touched_paths.and_then(|paths| active_root_ids_for_paths(repo_root, config, paths));
    let compile_result = match active_root_ids.as_ref() {
        Some(root_ids) => {
            run_structural_compile_for_root_ids(repo_root, config, &mut graph, root_ids)
        }
        None => run_structural_compile(repo_root, config, &mut graph),
    };

    match compile_result {
        Ok(summary) => {
            if !fast {
                if let Err(err) = emit_cochange_edges_pass(repo_root, config, &mut graph) {
                    tracing::warn!(error = %err, "co-change edge emission failed; continuing");
                }
                if let Err(err) = emit_symbol_revisions_pass(repo_root, config, &mut graph) {
                    tracing::warn!(error = %err, "symbol revision derivation failed; continuing");
                }
            }
            let repo_index_strategy = match touched_paths.filter(|paths| !paths.is_empty()) {
                Some(paths) => RepoIndexStrategy::Incremental(paths),
                None => RepoIndexStrategy::FullRebuild,
            };
            if let Err(err) =
                finish_runtime_surfaces(repo_root, config, synrepo_dir, &graph, repo_index_strategy)
            {
                return ReconcileOutcome::Failed(format!("surface maintenance failed: {err}"));
            }
            ReconcileOutcome::Completed(summary)
        }
        Err(err) => ReconcileOutcome::Failed(err.to_string()),
    }
}

fn active_root_ids_for_paths(
    repo_root: &Path,
    config: &Config,
    touched_paths: &[PathBuf],
) -> Option<std::collections::BTreeSet<String>> {
    let roots = crate::substrate::discover_roots(repo_root, config);
    let mut active = std::collections::BTreeSet::new();
    for path in touched_paths {
        let normalized = canonicalize_event_path(path);
        let owner = roots
            .iter()
            .filter(|root| normalized.starts_with(&root.absolute_path))
            .max_by_key(|root| root.absolute_path.as_os_str().len());
        if let Some(root) = owner {
            active.insert(root.discriminant.clone());
        }
    }
    (!active.is_empty()).then_some(active)
}

fn canonicalize_event_path(path: &Path) -> PathBuf {
    path.canonicalize()
        .ok()
        .or_else(|| {
            let name = path.file_name()?;
            let parent = path.parent()?;
            let canonical_parent = parent.canonicalize().ok()?;
            Some(canonical_parent.join(name))
        })
        .unwrap_or_else(|| path.to_path_buf())
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

    graph.begin()?;
    if let Err(err) = (|| -> crate::Result<()> {
        graph.delete_edges_by_kind(EdgeKind::CoChangesWith)?;
        for root in crate::substrate::discover_roots(repo_root, config) {
            let context = GitIntelligenceContext::inspect(&root.absolute_path, config);
            let max_commits = config.git_commit_depth as usize;
            let insights = analyze_recent_history(&context, max_commits, 100)?;
            let revision = insights.history.status.source_revision.clone();
            let file_index = file_index_for_root(graph, &root.discriminant)?;
            emit_cochange_edges(graph, &insights, &file_index, &revision)?;
        }
        Ok(())
    })() {
        let _ = graph.rollback();
        return Err(err);
    }
    graph.commit()?;
    Ok(())
}

/// Derive per-symbol `first_seen_rev` and `last_modified_rev` from body-hash
/// transitions in sampled git history. Runs in its own transaction so a
/// failure does not affect the structural compile or co-change emission.
pub fn emit_symbol_revisions_pass(
    repo_root: &Path,
    config: &Config,
    graph: &mut dyn crate::structure::graph::GraphStore,
) -> crate::Result<()> {
    use crate::pipeline::git::GitIntelligenceContext;
    use crate::pipeline::git_intelligence::derive_symbol_revisions_for_root;

    let max_commits = config.git_commit_depth as usize;

    graph.begin()?;
    if let Err(err) = (|| -> crate::Result<()> {
        for root in crate::substrate::discover_roots(repo_root, config) {
            let context = GitIntelligenceContext::inspect(&root.absolute_path, config);
            derive_symbol_revisions_for_root(
                &root.absolute_path,
                &context,
                graph,
                max_commits,
                &root.discriminant,
            )?;
        }
        Ok(())
    })() {
        let _ = graph.rollback();
        return Err(err);
    }
    graph.commit()?;
    Ok(())
}

fn file_index_for_root(
    graph: &dyn crate::structure::graph::GraphStore,
    root_id: &str,
) -> crate::Result<std::collections::HashMap<String, crate::core::ids::FileNodeId>> {
    let mut file_index = std::collections::HashMap::new();
    for (_, file_id) in graph.all_file_paths()? {
        let Some(file) = graph.get_file(file_id)? else {
            continue;
        };
        if file.root_id == root_id {
            file_index.insert(file.path, file.id);
        }
    }
    Ok(file_index)
}
