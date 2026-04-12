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

use super::{Budget, CardCompiler, FileCard, FileRef, SourceStore, SymbolCard, SymbolRef};
use crate::{
    core::ids::{FileNodeId, NodeId, SymbolNodeId},
    overlay::OverlayStore,
    pipeline::synthesis::CommentaryGenerator,
    structure::graph::GraphStore,
};

mod file;
mod io;
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
        symbol::symbol_card(
            symbol::SymbolCardContext {
                graph: self.graph.as_ref(),
                repo_root: &self.repo_root,
                overlay: self.overlay.as_ref(),
                generator: self.generator.as_ref(),
            },
            id,
            budget,
        )
    }

    fn file_card(&self, id: FileNodeId, budget: Budget) -> crate::Result<FileCard> {
        file::file_card(self.graph.as_ref(), id, budget)
    }

    fn resolve_target(&self, target: &str) -> crate::Result<Option<NodeId>> {
        resolve::resolve_target(self.graph.as_ref(), target)
    }
}
