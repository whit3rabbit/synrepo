//! `GraphCardCompiler`: the primary implementation of `CardCompiler` backed
//! by the `SqliteGraphStore`-compatible `GraphStore` trait.
//!
//! ## Phase-1 limitations
//!
//! - `SymbolCard.callers` and `.callees` are empty: stage-4 edges are
//!   file→symbol, not symbol→symbol. Symbol-level call resolution is stage 5+.
//! - `FileCard.co_changes` is empty until stage 5 (git mining) is wired.
//! - `FileCard.git_intelligence` is `None` until `git-intelligence-v1`.
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

use super::{
    Budget, CardCompiler, EntryPointCard, FileCard, FileRef, ModuleCard, SourceStore, SymbolCard,
    SymbolRef,
};
use crate::{
    core::ids::{FileNodeId, NodeId, SymbolNodeId},
    overlay::OverlayStore,
    pipeline::synthesis::CommentaryGenerator,
    structure::graph::{with_graph_read_snapshot, GraphStore},
};

mod entry_point;
mod file;
mod io;
mod links;
mod module;
mod resolve;
mod symbol;

#[cfg(test)]
mod tests;

/// A `CardCompiler` backed by a `GraphStore` reference.
///
/// Holds an optional overlay store + commentary generator pair; when both
/// are absent, commentary resolution is a no-op.
pub struct GraphCardCompiler {
    graph: Box<dyn GraphStore>,
    /// Repository root, used to read source bodies at `Deep` budget.
    repo_root: Option<PathBuf>,
    /// Optional overlay store for commentary retrieval and persistence.
    overlay: Option<Arc<parking_lot::Mutex<dyn OverlayStore>>>,
    /// Optional generator invoked lazily at `Deep` budget when no overlay
    /// entry yet exists for the target node.
    generator: Option<Arc<dyn CommentaryGenerator>>,
}

impl GraphCardCompiler {
    /// Create a compiler from a boxed graph store.
    ///
    /// Pass `repo_root` to enable source-body inclusion at `Deep` budget.
    pub fn new(graph: Box<dyn GraphStore>, repo_root: Option<impl Into<PathBuf>>) -> Self {
        Self {
            graph,
            repo_root: repo_root.map(Into::into),
            overlay: None,
            generator: None,
        }
    }

    /// Attach an optional overlay store and generator for commentary.
    pub fn with_overlay(
        mut self,
        overlay: Option<Arc<parking_lot::Mutex<dyn OverlayStore>>>,
        generator: Option<Arc<dyn CommentaryGenerator>>,
    ) -> Self {
        self.overlay = overlay;
        self.generator = generator;
        self
    }

    /// Access the underlying graph store for direct queries.
    pub fn graph(&self) -> &dyn GraphStore {
        self.graph.as_ref()
    }
}

impl CardCompiler for GraphCardCompiler {
    fn symbol_card(&self, id: SymbolNodeId, budget: Budget) -> crate::Result<SymbolCard> {
        // Pin a single committed epoch on the graph for the whole compile.
        // The overlay is intentionally NOT wrapped here: the Deep-budget
        // commentary path may lazily write a freshly generated entry via
        // `insert_commentary`, and mixing that write into an outer read
        // snapshot would silently upgrade the snapshot to a write
        // transaction. Overlay reads use per-statement auto-commit; any
        // brief inconsistency is cosmetic rather than structural.
        with_graph_read_snapshot(self.graph.as_ref(), |graph| {
            symbol::symbol_card(
                symbol::SymbolCardContext {
                    graph,
                    repo_root: &self.repo_root,
                    overlay: self.overlay.as_ref(),
                    generator: self.generator.as_ref(),
                },
                id,
                budget,
            )
        })
    }

    fn file_card(&self, id: FileNodeId, budget: Budget) -> crate::Result<FileCard> {
        with_graph_read_snapshot(self.graph.as_ref(), |graph| {
            file::file_card(graph, self.overlay.as_ref(), id, budget)
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

    fn resolve_target(&self, target: &str) -> crate::Result<Option<NodeId>> {
        with_graph_read_snapshot(self.graph.as_ref(), |graph| {
            resolve::resolve_target(graph, target)
        })
    }
}
