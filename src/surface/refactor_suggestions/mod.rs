//! Deterministic refactor-suggestion candidates for source files.

use std::path::Path;

use crate::config::Config;
use crate::store::sqlite::SqliteGraphStore;
use crate::substrate::{discover_roots, DiscoveryRoot};
use crate::surface::card::compiler::GraphCardCompiler;

mod line_count;
mod missing_docs;
mod types;
mod util;

#[cfg(test)]
mod tests;

pub use types::{
    MissingPublicDocSymbol, RefactorSuggestionCandidate, RefactorSuggestionCriteria,
    RefactorSuggestionGroup, RefactorSuggestionMode, RefactorSuggestionOptions,
    RefactorSuggestionReport, RefactorSymbolCounts, DEFAULT_LIMIT, DEFAULT_MIN_LINES,
    DEFAULT_MISSING_PUBLIC_DOC_PREVIEW_LIMIT, METRIC_MISSING_PUBLIC_DOCS, METRIC_PHYSICAL_LINES,
    SOURCE_STORE,
};

/// Collect refactor suggestions for a repository by opening its graph store.
pub fn collect_refactor_suggestions_for_repo(
    repo_root: &Path,
    options: RefactorSuggestionOptions,
) -> crate::Result<RefactorSuggestionReport> {
    let config = Config::load(repo_root)?;
    let graph_dir = Config::synrepo_dir(repo_root).join("graph");
    let graph = SqliteGraphStore::open_existing(&graph_dir)?;
    let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo_root.to_path_buf()))
        .with_config(config.clone());
    let roots = discover_roots(repo_root, &config);
    collect_refactor_suggestions(&compiler, &roots, options)
}

/// Collect refactor suggestions from an existing graph-card compiler.
pub fn collect_refactor_suggestions(
    compiler: &GraphCardCompiler,
    roots: &[DiscoveryRoot],
    options: RefactorSuggestionOptions,
) -> crate::Result<RefactorSuggestionReport> {
    compiler.with_reader(|reader| match options.mode {
        RefactorSuggestionMode::LineCount => line_count::collect(reader, roots, &options),
        RefactorSuggestionMode::MissingDocs => missing_docs::collect(reader, roots, &options),
    })
}
