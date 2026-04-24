//! Stage 4: cross-file edge resolution.
//!
//! Runs inside the same transaction as stages 1–3. Builds an in-memory name
//! index from the graph (SQLite read-your-own-writes sees the uncommitted nodes
//! from stages 1–3 on the same connection), then emits `Calls` and `Imports`
//! edges for newly parsed files. The caller owns the transaction; this module
//! never calls begin or commit.
//!
//! ## Approximate resolution contract (scoped, v2)
//!
//! Call sites are resolved using a scoring rubric that considers:
//! - Same file (+100): always callable.
//! - Imported file (+50): strong positive signal.
//! - Visibility (+20 Public, +10 Crate, -100 Private cross-file).
//! - Kind match (+30): method call ↔ Method, free call ↔ Function/Constant.
//! - Prefix match (+40): callee_prefix matches a component of candidate's qname.
//!
//! Cutoff rules:
//! - Top score ≤ 0: drop (no candidate scores positive).
//! - Unique top score: emit edge to that candidate.
//! - Multiple tied at top score ≥ 50: emit edges to all (scoped ambiguity).
//! - Multiple tied at top score < 50: drop (weak ambiguity).
//!
//! Import paths resolved as before.
//!
//! ## Resolver lookups use the graph's `file_index`, not the filesystem
//!
//! Rust top-level-name checks and Go package fan-out both enumerate files via
//! the in-memory `file_index` / `files_by_dir` built from `all_file_paths()`.
//! This guarantees the resolver's view matches the graph (respecting
//! `.gitignore` and redactions) and avoids one syscall per import.

use std::{collections::HashSet, path::Path};

mod context;
mod imports;
mod rust_paths;
mod scoring;

pub use context::CrossFilePending;

use context::{build_indices, ImportsMap};
use imports::emit_imports_for_file;
use scoring::emit_calls_for_file;

use crate::{
    core::ids::{FileNodeId, SymbolNodeId},
    structure::graph::GraphStore,
};

/// Run stage 4: build the global name/file index and emit cross-file edges.
///
/// Returns the number of new edges emitted.
pub fn run_cross_file_resolution(
    graph: &mut dyn GraphStore,
    pending: &[CrossFilePending],
    revision: &str,
    repo_root: &Path,
) -> crate::Result<usize> {
    if pending.is_empty() {
        return Ok(0);
    }

    let (ctx, name_index, symbol_meta) = build_indices(graph, pending, repo_root)?;

    // Imports map: populated as Imports edges are emitted, before Calls resolution.
    // Maps importing_file -> set of imported file IDs.
    let mut imports_map = ImportsMap::new();

    // Edge insertions run inside the caller's open transaction; no begin/commit here.
    let mut emitted = 0usize;

    let empty_imports: HashSet<FileNodeId> = HashSet::new();
    let mut scored: Vec<(SymbolNodeId, i32)> = Vec::new();

    // Global call-resolution counters (accumulated per-file).
    let mut total_calls_resolved_uniquely = 0usize;
    let mut total_calls_resolved_ambiguously = 0usize;
    let mut total_calls_dropped_weak = 0usize;
    let mut total_calls_dropped_no_candidates = 0usize;

    for item in pending {
        emitted += emit_imports_for_file(graph, &ctx, item, revision, &mut imports_map)?;

        let imports = imports_map.get(&item.file_id).unwrap_or(&empty_imports);

        let call_stats = emit_calls_for_file(
            graph,
            &name_index,
            &symbol_meta,
            item,
            imports,
            revision,
            &mut scored,
        )?;
        emitted += call_stats.emitted_edges();

        // Accumulate into global counters.
        total_calls_resolved_uniquely += call_stats.calls_resolved_uniquely;
        total_calls_resolved_ambiguously += call_stats.calls_resolved_ambiguously;
        total_calls_dropped_weak += call_stats.calls_dropped_weak;
        total_calls_dropped_no_candidates += call_stats.calls_dropped_no_candidates;
    }

    // Global telemetry rollup.
    tracing::trace!(
        total_calls_resolved_uniquely,
        total_calls_resolved_ambiguously,
        total_calls_dropped_weak,
        total_calls_dropped_no_candidates,
        "stage4 call-resolution global summary"
    );

    Ok(emitted)
}
