//! The structural compile pipeline.
//!
//! Runs synchronously, LLM-free, on every `synrepo init` and on-demand
//! refresh. This initial producer set covers stages 1–3 of the full
//! eight-stage pipeline:
//!
//! 1. **Discover** — walk the repo via `substrate::discover`, reusing its
//!    `.gitignore` / `.synignore` / redaction rules.
//! 2. **Parse code** — tree-sitter for each `SupportedCode` file, extract
//!    symbols and within-file `defines` edges.
//! 3. **Parse prose** — markdown link parser for files in configured concept
//!    directories, extract concept nodes.
//!
//! Stage 4 (cross-file edge resolution) is now wired: after stages 1–3
//! commit, a name-resolution pass emits `Calls` and `Imports` edges.
//! Stages 5–8 (git mining, identity cascade, drift scoring, ArcSwap commit)
//! remain TODO stubs.
//!
//! ## Relationship to watch and reconcile
//!
//! The watcher (`pipeline::watch`) drives this function as a trigger-and-
//! coalesce layer rather than as an independent graph producer. Each
//! reconcile pass calls `run_structural_compile` under the writer lock
//! (`pipeline::writer`). This function should remain stateless and
//! re-entrant so the reconcile path can call it safely on any event burst.
//!
//! ## Observation lifecycle
//!
//! Each compile run refreshes the producer-owned observation slice:
//! - File nodes with changed content are upserted (preserving FileNodeId),
//!   their symbols are diffed, and stale symbols/edges are soft-retired.
//! - File nodes whose paths have disappeared from the discovered set are
//!   physically deleted (cascade).
//! - Concept nodes whose paths have disappeared are deleted.
//! - The run is idempotent, unchanged files are skipped entirely.

mod ids;
pub use crate::structure::graph::derive_edge_id;

mod provenance;
mod stage4;
mod stages;

#[cfg(test)]
mod tests;

use std::{collections::BTreeSet, path::Path, time::Instant};

use crate::{
    config::Config,
    structure::{drift, graph::GraphStore},
    substrate,
};

use stages::stages_1_to_3;

/// Run one structural compile cycle.
///
/// Re-entrant and idempotent, calling twice with the same repository state
/// produces the same graph contents both times.
pub fn run_structural_compile(
    repo_root: &Path,
    config: &Config,
    graph: &mut dyn GraphStore,
) -> crate::Result<CompileSummary> {
    let start = Instant::now();

    let discovered = substrate::discover(repo_root, config)?;
    let files_discovered = discovered.len();

    let discovered_paths: BTreeSet<String> =
        discovered.iter().map(|f| f.relative_path.clone()).collect();

    // All four stages run inside a single transaction so readers never observe
    // a partially-compiled graph (nodes present but cross-file edges absent).
    // SQLite read-your-own-writes: stage 4's symbol/file queries see the nodes
    // inserted by stages 1–3 on the same connection without an intermediate commit.
    // On any error in any stage, rollback leaves the graph in its prior state.
    // Allocate a compile revision for observation-window tracking.
    // Falls back to None for test stores that don't implement it.
    let compile_rev = match graph.next_compile_revision() {
        Ok(0) => None, // test store default
        Ok(rev) => Some(rev),
        Err(_) => None,
    };

    graph.begin()?;
    let (txn, stage4_edges) = match (|| -> crate::Result<_> {
        let txn = stages_1_to_3(
            repo_root,
            config,
            graph,
            &discovered,
            &discovered_paths,
            compile_rev,
        )?;
        let edges =
            stage4::run_cross_file_resolution(graph, &txn.cross_file_pending, &txn.revision)?;
        Ok((txn, edges))
    })() {
        Ok(v) => v,
        Err(e) => {
            let _ = graph.rollback();
            return Err(e);
        }
    };
    graph.commit()?;

    // Stage 7: drift scoring (runs outside the main transaction since it
    // only writes to the sidecar edge_drift table, not the graph itself).
    let drift_scored = match drift::run_drift_scoring(graph, &txn.revision) {
        Ok(count) => count,
        Err(e) => {
            tracing::warn!(error = %e, "stage 7 drift scoring failed; continuing");
            0
        }
    };

    Ok(CompileSummary {
        files_discovered,
        files_parsed: txn.files_parsed,
        symbols_extracted: txn.symbols_extracted,
        edges_added: txn.edges_added + stage4_edges,
        concept_nodes_emitted: txn.concept_nodes_emitted,
        identities_resolved: txn.identities_resolved,
        drift_scored,
        elapsed_ms: start.elapsed().as_millis() as u64,
    })
}

/// Summary of what one compile cycle produced.
#[derive(Clone, Debug, Default)]
pub struct CompileSummary {
    /// Files discovered and classified.
    pub files_discovered: usize,
    /// Files parsed for code symbols.
    pub files_parsed: usize,
    /// Symbols extracted across all parsed files.
    pub symbols_extracted: usize,
    /// `defines` edges added this cycle.
    pub edges_added: usize,
    /// Concept nodes emitted from markdown files.
    pub concept_nodes_emitted: usize,
    /// Identity resolutions performed (phase-1+).
    pub identities_resolved: usize,
    /// Edges that received a non-zero drift score this cycle.
    pub drift_scored: usize,
    /// Wall-clock time in milliseconds.
    pub elapsed_ms: u64,
}
