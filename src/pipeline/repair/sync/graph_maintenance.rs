//! Graph-maintenance repair handlers.

use std::path::Path;

use crate::pipeline::repair::RepairFinding;
use crate::store::sqlite::SqliteGraphStore;
use crate::structure::graph::{GraphReader, GraphStore};

use super::handlers::ActionContext;

/// Prune edges with drift score of 1.0 (dead edges).
pub(super) fn prune_dead_edges(
    finding: &RepairFinding,
    synrepo_dir: &Path,
    repaired: &mut Vec<RepairFinding>,
    actions_taken: &mut Vec<String>,
) -> crate::Result<()> {
    let graph_dir = synrepo_dir.join("graph");
    let Ok(mut graph) = SqliteGraphStore::open_existing(&graph_dir) else {
        actions_taken.push("edge drift pruning skipped: graph store not found".to_string());
        return Ok(());
    };

    // Use the latest revision recorded in edge_drift.
    let Some(revision) = graph.latest_drift_revision()? else {
        return Ok(());
    };

    let scores = graph.read_drift_scores(&revision)?;
    let dead: Vec<_> = scores
        .iter()
        .filter(|(_, score)| (*score - 1.0).abs() < f32::EPSILON)
        .collect();

    if dead.is_empty() {
        return Ok(());
    }

    graph.begin()?;
    let mut pruned = 0usize;
    let mut failed = 0usize;
    let mut last_err: Option<crate::Error> = None;
    for (edge_id, _) in &dead {
        match graph.delete_edge(*edge_id) {
            Ok(()) => pruned += 1,
            Err(err) => {
                tracing::warn!(edge_id = %edge_id, error = %err, "delete_edge failed during prune_dead_edges");
                failed += 1;
                last_err = Some(err);
            }
        }
    }

    let total = dead.len();
    if failed == 0 {
        graph.commit()?;
        actions_taken.push(format!("pruned {pruned} dead edges (drift 1.0)"));
        repaired.push(finding.clone());
        Ok(())
    } else if pruned > 0 {
        graph.commit()?;
        // Why: partial success means operators need the failed count, but an
        // error return would hide the partial progress signal.
        actions_taken.push(format!(
            "pruned {pruned}/{total} dead edges (drift 1.0); {failed} failure(s)"
        ));
        repaired.push(finding.clone());
        Ok(())
    } else {
        let _ = graph.rollback();
        Err(last_err.unwrap_or_else(|| {
            crate::Error::Other(anyhow::anyhow!(
                "prune_dead_edges: all {total} delete_edge calls failed"
            ))
        }))
    }
}

/// Run compaction on retired observations.
pub(super) fn compact_retired_observations(
    context: &ActionContext<'_>,
    actions_taken: &mut Vec<String>,
) -> crate::Result<()> {
    let graph_dir = context.synrepo_dir.join("graph");
    let mut graph = SqliteGraphStore::open_existing(&graph_dir)?;

    let current_rev = graph.next_compile_revision()?;
    let retain = context.config.retain_retired_revisions;
    if current_rev <= retain {
        actions_taken.push("compaction skipped: not enough revisions yet".to_string());
        return Ok(());
    }
    let threshold = current_rev - retain;
    let summary = graph.compact_retired(threshold)?;

    actions_taken.push(format!(
        "compaction: removed {} retired symbols, {} retired edges, {} old revisions",
        summary.symbols_removed, summary.edges_removed, summary.revisions_removed
    ));
    Ok(())
}
