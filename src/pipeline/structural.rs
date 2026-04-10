//! The structural compile pipeline.
//!
//! Runs on every change, synchronously, LLM-free. Walks the configured
//! roots, parses code via tree-sitter, parses prose via the markdown
//! parser, mines git history, computes drift scores, commits to sqlite
//! and syntext. This is the entire critical path — no cascade budget,
//! no deferral, no nightly queue.

use crate::config::Config;
use crate::structure::graph::GraphStore;
use std::path::Path;

/// Run one full structural compile cycle.
///
/// Phase 0/1 skeleton — real implementation is a multi-stage pipeline:
///
/// 1. Discover: walk the filesystem, classify files, apply .gitignore and redaction.
/// 2. Parse code: tree-sitter for each supported file; extract symbols and within-file edges.
/// 3. Parse prose: markdown link parser for every .md/.mdx; extract wiki-links, frontmatter, ADR governance declarations.
/// 4. Resolve cross-file edges: match symbol references across files using qualified names.
/// 5. Git mine: walk the last N commits for co-change, blame, ownership.
/// 6. Identity: run the rename detection cascade on any disappeared files.
/// 7. Drift: recompute drift scores for all edges whose endpoints changed.
/// 8. Commit: atomically publish the new state via ArcSwap.
pub fn run_structural_compile(
    repo_root: &Path,
    _config: &Config,
    _graph: &mut dyn GraphStore,
) -> crate::Result<CompileSummary> {
    // TODO(phase-0): implement stages 1-2.
    // TODO(phase-1): implement stages 3-8.
    let _ = repo_root;
    Ok(CompileSummary::default())
}

/// Summary of what one compile cycle did. Reported via tracing and the CLI.
#[derive(Clone, Debug, Default)]
pub struct CompileSummary {
    /// Files discovered and classified.
    pub files_discovered: usize,
    /// Files parsed for symbols.
    pub files_parsed: usize,
    /// Symbols extracted.
    pub symbols_extracted: usize,
    /// Edges added this cycle.
    pub edges_added: usize,
    /// Identity resolutions performed.
    pub identities_resolved: usize,
    /// Wall-clock time in milliseconds.
    pub elapsed_ms: u64,
}