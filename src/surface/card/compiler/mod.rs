//! `GraphCardCompiler`: the primary implementation of `CardCompiler` backed
//! by the `SqliteGraphStore`-compatible `GraphStore` trait.
//!
//! ## Phase-1 limitations
//!
//! - `SymbolCard.callers` and `.callees` are empty: stage-4 edges are
//!   file→symbol, not symbol→symbol. Symbol-level call resolution is stage 5+.
//!
//! ## Git intelligence
//!
//! When the compiler is constructed with [`GraphCardCompiler::with_config`]
//! and a `repo_root`, `FileCard.git_intelligence` and `SymbolCard.last_change`
//! are populated at `Normal` and `Deep` budgets via `analyze_path_history`.
//! Results are cached on the compiler so multiple cards against the same
//! file share one git walk per compile session.
//!
//! ## Overlay commentary
//!
//! Constructed via [`GraphCardCompiler::with_overlay`] the compiler can
//! populate `SymbolCard.overlay_commentary` at `Deep` budget. When no
//! overlay is wired, commentary resolution is skipped and the flat
//! `commentary_state` field reflects the actual absence reason
//! (`missing` at Deep, `budget_withheld` at Tiny/Normal).

use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;

use super::{
    Budget, CardCompiler, ChangeRiskCard, EntryPointCard, FileCard, FileRef, ModuleCard,
    PublicAPICard, SourceStore, SymbolCard, SymbolRef,
};
use crate::{
    config::Config,
    core::ids::{FileNodeId, NodeId, SymbolNodeId},
    overlay::OverlayStore,
    pipeline::{git_intelligence::GitPathHistoryInsights, synthesis::CommentaryGenerator},
    structure::graph::{with_graph_read_snapshot, GraphStore},
};

use self::git_cache::GitCache;

mod call_path;
mod change_risk;
mod entry_point;
mod file;
mod git_cache;
mod io;
mod links;
mod module;
pub mod neighborhood;
mod public_api;
mod resolve;
mod symbol;
mod test_surface;

#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;

/// A `CardCompiler` backed by a `GraphStore` reference.
///
/// Holds an optional overlay store + commentary generator pair; when both
/// are absent, commentary resolution is a no-op.
pub struct GraphCardCompiler {
    graph: Box<dyn GraphStore>,
    /// Repository root, used to read source bodies at `Deep` budget and to
    /// scope git-intelligence lookups.
    repo_root: Option<PathBuf>,
    /// Optional runtime config. When paired with `repo_root`, the compiler
    /// populates git-derived card fields using `git_commit_depth`. When
    /// absent, git-derived fields stay `None`.
    config: Option<Config>,
    /// Per-compiler cache of path-scoped git analyses. Opens the
    /// `GitIntelligenceContext` at most once per HEAD generation, serves
    /// reads through an internal `RwLock`, and rebuilds the underlying
    /// history index when the repository's HEAD SHA moves.
    git_cache: GitCache,
    /// Optional overlay store for commentary retrieval and persistence.
    overlay: Option<Arc<Mutex<dyn OverlayStore>>>,
}

impl GraphCardCompiler {
    /// Create a compiler from a boxed graph store.
    ///
    /// Pass `repo_root` to enable source-body inclusion at `Deep` budget.
    pub fn new(graph: Box<dyn GraphStore>, repo_root: Option<impl Into<PathBuf>>) -> Self {
        Self {
            graph,
            repo_root: repo_root.map(Into::into),
            config: None,
            git_cache: GitCache::new(),
            overlay: None,
        }
    }

    /// Attach a runtime `Config`, enabling git-intelligence population on
    /// cards at `Normal` and `Deep` budgets. Without this builder the
    /// compiler treats git-derived fields as unavailable.
    pub fn with_config(mut self, config: Config) -> Self {
        self.config = Some(config);
        self
    }

    /// Attach an optional overlay store for commentary.
    pub fn with_overlay(mut self, overlay: Option<Arc<Mutex<dyn OverlayStore>>>) -> Self {
        self.overlay = overlay;
        self
    }

    /// Access the underlying graph store for direct queries.
    pub fn graph(&self) -> &dyn GraphStore {
        self.graph.as_ref()
    }

    /// Resolve (and cache) git-intelligence for a repo-relative path.
    ///
    /// Returns `None` when the compiler was not configured with both a
    /// `repo_root` and a `Config`, when the repo has no git, or when the
    /// underlying analysis errors. Failures are swallowed at `debug` level
    /// rather than propagated — git-derived card fields are best-effort,
    /// not load-bearing.
    pub(crate) fn resolve_file_git_intelligence(
        &self,
        path: &str,
    ) -> Option<Arc<GitPathHistoryInsights>> {
        let repo_root = self.repo_root.as_ref()?;
        let config = self.config.as_ref()?;
        self.git_cache.resolve_path(repo_root, config, path)
    }

    /// Explicitly refresh commentary for a node using the provided generator.
    ///
    /// This is the only path that writes to the overlay for commentary. It
    /// compiles the target card at `Deep` budget to build context, calls
    /// the generator, and persists the result. Returns the generated text
    /// on success.
    pub fn refresh_commentary(
        &self,
        id: NodeId,
        generator: &dyn CommentaryGenerator,
    ) -> crate::Result<Option<String>> {
        let overlay = self.overlay.as_ref().ok_or_else(|| {
            crate::Error::Other(anyhow::anyhow!("no overlay store configured for refresh"))
        })?;

        match id {
            NodeId::Symbol(sym_id) => {
                // Pin a graph snapshot to read the card context safely.
                let (prompt, content_hash) =
                    with_graph_read_snapshot(self.graph.as_ref(), |graph| {
                        let card = symbol::symbol_card(
                            symbol::SymbolCardContext {
                                compiler: self,
                                graph,
                                repo_root: &self.repo_root,
                                overlay: self.overlay.as_ref(),
                            },
                            sym_id,
                            Budget::Deep,
                        )?;

                        let symbol = graph.get_symbol(sym_id)?.ok_or_else(|| {
                            crate::Error::Other(anyhow::anyhow!("symbol {sym_id} not found"))
                        })?;
                        let file = graph.get_file(symbol.file_id)?.ok_or_else(|| {
                            crate::Error::Other(anyhow::anyhow!(
                                "file for symbol {sym_id} not found"
                            ))
                        })?;

                        Ok((
                            symbol::build_generation_context(&card),
                            file.content_hash.clone(),
                        ))
                    })?;

                match generator.generate(id, &prompt)? {
                    Some(mut entry) => {
                        // Fill in the source content hash so the entry is immediately Fresh.
                        entry.provenance.source_content_hash = content_hash;
                        let text = entry.text.clone();
                        overlay.lock().insert_commentary(entry)?;
                        Ok(Some(text))
                    }
                    None => Ok(None),
                }
            }
            _ => {
                tracing::debug!(node_id = %id, "commentary refresh requested for unsupported node kind");
                Ok(None)
            }
        }
    }
}

impl CardCompiler for GraphCardCompiler {
    fn symbol_card(&self, id: SymbolNodeId, budget: Budget) -> crate::Result<SymbolCard> {
        // Pin a single committed epoch on the graph for the whole compile.
        with_graph_read_snapshot(self.graph.as_ref(), |graph| {
            symbol::symbol_card(
                symbol::SymbolCardContext {
                    compiler: self,
                    graph,
                    repo_root: &self.repo_root,
                    overlay: self.overlay.as_ref(),
                },
                id,
                budget,
            )
        })
    }

    fn file_card(&self, id: FileNodeId, budget: Budget) -> crate::Result<FileCard> {
        with_graph_read_snapshot(self.graph.as_ref(), |graph| {
            file::file_card(self, graph, id, budget)
        })
    }

    fn entry_point_card(
        &self,
        scope: Option<&str>,
        budget: Budget,
    ) -> crate::Result<EntryPointCard> {
        with_graph_read_snapshot(self.graph.as_ref(), |graph| {
            entry_point::entry_point_card_impl(graph, scope, budget)
        })
    }

    fn module_card(&self, path: &str, budget: Budget) -> crate::Result<ModuleCard> {
        with_graph_read_snapshot(self.graph.as_ref(), |graph| {
            module::module_card_impl(graph, path, budget)
        })
    }

    fn public_api_card(&self, path: &str, budget: Budget) -> crate::Result<PublicAPICard> {
        with_graph_read_snapshot(self.graph.as_ref(), |graph| {
            public_api::public_api_card_impl(self, graph, path, budget)
        })
    }

    fn call_path_card(
        &self,
        target: SymbolNodeId,
        budget: Budget,
    ) -> crate::Result<super::CallPathCard> {
        with_graph_read_snapshot(self.graph.as_ref(), |graph| {
            call_path::call_path_card_impl(graph, target, budget)
        })
    }

    fn test_surface_card(
        &self,
        scope: &str,
        budget: Budget,
    ) -> crate::Result<super::TestSurfaceCard> {
        with_graph_read_snapshot(self.graph.as_ref(), |graph| {
            test_surface::test_surface_card_impl(graph, scope, budget)
        })
    }

    fn resolve_target(&self, target: &str) -> crate::Result<Option<NodeId>> {
        with_graph_read_snapshot(self.graph.as_ref(), |graph| {
            resolve::resolve_target(graph, target)
        })
    }

    fn change_risk_card(&self, target: NodeId, budget: Budget) -> crate::Result<ChangeRiskCard> {
        with_graph_read_snapshot(self.graph.as_ref(), |_graph| {
            change_risk::change_risk_card(self, target, budget)
        })
    }
}
