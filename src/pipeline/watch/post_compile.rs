//! Shared post-compile surface maintenance for bootstrap and reconcile paths.

use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::core::ids::NodeId;
use crate::overlay::OverlayStore;
use crate::pipeline::explain::docs::{reconcile_commentary_docs, sync_commentary_index};
use crate::store::overlay::SqliteOverlayStore;
use crate::store::sqlite::SqliteGraphStore;

/// Repo lexical-index maintenance mode after a successful structural compile.
pub(crate) enum RepoIndexStrategy<'a> {
    /// Skip repo-index maintenance because the caller already rebuilt it.
    Skip,
    /// Rebuild the repo index from scratch.
    FullRebuild,
    /// Apply an incremental touched-path update.
    Incremental(&'a [PathBuf]),
}

/// Finish non-graph runtime surfaces after a successful structural compile.
pub(crate) fn finish_runtime_surfaces(
    repo_root: &Path,
    config: &Config,
    synrepo_dir: &Path,
    graph: &SqliteGraphStore,
    repo_index_strategy: RepoIndexStrategy<'_>,
) -> crate::Result<()> {
    match repo_index_strategy {
        RepoIndexStrategy::Skip => {}
        RepoIndexStrategy::FullRebuild => {
            crate::substrate::build_index(config, repo_root)?;
        }
        RepoIndexStrategy::Incremental(paths) => {
            crate::substrate::sync_index_incremental(config, repo_root, paths)?;
        }
    }

    prune_overlay_orphans(synrepo_dir, graph);
    sync_commentary_surfaces(synrepo_dir, graph)?;
    Ok(())
}

fn sync_commentary_surfaces(synrepo_dir: &Path, graph: &SqliteGraphStore) -> crate::Result<()> {
    let overlay_dir = synrepo_dir.join("overlay");
    let overlay = SqliteOverlayStore::open_existing(&overlay_dir).ok();
    let touched = reconcile_commentary_docs(synrepo_dir, graph, overlay.as_ref())?;
    sync_commentary_index(synrepo_dir, &touched)?;
    Ok(())
}

fn prune_overlay_orphans(synrepo_dir: &Path, graph: &SqliteGraphStore) {
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
        Ok(overlay) => overlay,
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
            );
        }
        Ok(_) => {}
        Err(err) => {
            tracing::warn!(error = %err, "overlay: prune failed");
        }
    }
}
