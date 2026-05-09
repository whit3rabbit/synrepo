//! `GraphCardCompiler`: the primary implementation of `CardCompiler` backed
//! by the `SqliteGraphStore`-compatible `GraphStore` / `GraphReader` traits.
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
    overlay::{FreshnessState, OverlayStore},
    pipeline::{
        explain::{
            context::{build_context_text, resolve_context_target, CommentaryContextOptions},
            CommentaryGenerator,
        },
        git_intelligence::GitPathHistoryInsights,
    },
    store::sqlite::SqliteGraphStore,
    structure::graph::{with_graph_read_snapshot, Graph, GraphReader, GraphStore},
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
pub use resolve::resolve_target;
mod symbol;
mod test_surface;

#[cfg(test)]
mod test_support;
#[cfg(test)]
pub(crate) mod tests;

/// A `CardCompiler` backed by a `GraphStore` reference.
///
/// Holds an optional overlay store + commentary generator pair; when both
/// are absent, commentary resolution is a no-op.
pub struct GraphCardCompiler {
    backend: GraphBackend,
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

enum GraphBackend {
    Sqlite(Box<dyn GraphStore>),
    Snapshot(Arc<Graph>),
}

impl GraphCardCompiler {
    /// Create a compiler from a boxed graph store.
    ///
    /// Pass `repo_root` to enable source-body inclusion at `Deep` budget.
    pub fn new(graph: Box<dyn GraphStore>, repo_root: Option<impl Into<PathBuf>>) -> Self {
        Self {
            backend: GraphBackend::Sqlite(graph),
            repo_root: repo_root.map(Into::into),
            config: None,
            git_cache: GitCache::new(),
            overlay: None,
        }
    }

    /// Create a compiler backed by the published in-memory graph snapshot.
    pub fn new_with_snapshot(graph: Arc<Graph>, repo_root: Option<impl Into<PathBuf>>) -> Self {
        Self {
            backend: GraphBackend::Snapshot(graph),
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

    /// Access the underlying SQLite graph store when this compiler is using it.
    pub fn graph(&self) -> Option<&dyn GraphStore> {
        match &self.backend {
            GraphBackend::Sqlite(graph) => Some(graph.as_ref()),
            GraphBackend::Snapshot(_) => None,
        }
    }

    /// Access the current compiler backend as a read-only graph reader.
    pub fn reader(&self) -> &dyn GraphReader {
        match &self.backend {
            GraphBackend::Sqlite(graph) => graph.as_ref(),
            GraphBackend::Snapshot(graph) => graph.as_ref(),
        }
    }

    pub(crate) fn with_reader<R>(
        &self,
        f: impl FnOnce(&dyn GraphReader) -> crate::Result<R>,
    ) -> crate::Result<R> {
        match &self.backend {
            GraphBackend::Sqlite(graph) => with_graph_read_snapshot(graph.as_ref(), f),
            GraphBackend::Snapshot(graph) => f(graph.as_ref()),
        }
    }

    pub(crate) fn read_drift_scores(
        &self,
        revision: &str,
    ) -> crate::Result<Vec<(crate::EdgeId, f32)>> {
        match &self.backend {
            GraphBackend::Sqlite(graph) => graph.read_drift_scores(revision),
            GraphBackend::Snapshot(_) => {
                let repo_root = self.repo_root.as_ref().ok_or_else(|| {
                    crate::Error::Other(anyhow::anyhow!(
                        "snapshot-backed compiler requires repo_root for drift-score reads"
                    ))
                })?;
                let graph_dir = Config::synrepo_dir(repo_root).join("graph");
                let graph = SqliteGraphStore::open_existing(&graph_dir)?;
                with_graph_read_snapshot(&graph, |_| graph.read_drift_scores(revision))
            }
        }
    }

    /// Latest revision stored in the `edge_drift` sidecar, or `None` if no
    /// drift scoring has run yet.
    pub(crate) fn latest_drift_revision(&self) -> crate::Result<Option<String>> {
        match &self.backend {
            GraphBackend::Sqlite(graph) => graph.latest_drift_revision(),
            GraphBackend::Snapshot(_) => {
                let repo_root = self.repo_root.as_ref().ok_or_else(|| {
                    crate::Error::Other(anyhow::anyhow!(
                        "snapshot-backed compiler requires repo_root for drift-score reads"
                    ))
                })?;
                let graph_dir = Config::synrepo_dir(repo_root).join("graph");
                let graph = SqliteGraphStore::open_existing(&graph_dir)?;
                graph.latest_drift_revision()
            }
        }
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

    pub(crate) fn source_root_for(&self, root_id: &str) -> Option<PathBuf> {
        let repo_root = self.repo_root.as_ref()?;
        if root_id == "primary" {
            return Some(repo_root.clone());
        }
        let config = self.config.as_ref()?;
        crate::substrate::discover_roots(repo_root, config)
            .into_iter()
            .find(|root| root.discriminant == root_id)
            .map(|root| root.absolute_path)
    }

    /// Explicitly refresh commentary for a node using the provided generator.
    ///
    /// This is the only path that writes to the overlay for commentary. It
    /// builds shared graph-backed context, calls the generator, and persists
    /// the result. Returns the generated text on success.
    pub fn refresh_commentary(
        &self,
        id: NodeId,
        generator: &dyn CommentaryGenerator,
    ) -> crate::Result<Option<String>> {
        let overlay = self.overlay.as_ref().ok_or_else(|| {
            crate::Error::Other(anyhow::anyhow!("no overlay store configured for refresh"))
        })?;

        if matches!(id, NodeId::Concept(_)) {
            tracing::debug!(node_id = %id, "commentary refresh requested for unsupported node kind");
            return Ok(None);
        }

        let repo_root = self.repo_root.as_ref().ok_or_else(|| {
            crate::Error::Other(anyhow::anyhow!(
                "commentary refresh requires a repository root"
            ))
        })?;
        let max_input_tokens = self
            .config
            .as_ref()
            .map(|config| config.commentary_cost_limit)
            .unwrap_or_else(|| Config::default().commentary_cost_limit);

        // Pin a graph snapshot to read the prompt context safely.
        let (prompt, content_hash, doc_metadata) = self.with_reader(|graph| {
            let target = resolve_context_target(graph, id)?
                .ok_or_else(|| crate::Error::Other(anyhow::anyhow!("node {id} not found")))?;
            let prompt = build_context_text(
                repo_root,
                graph,
                &target,
                CommentaryContextOptions {
                    max_input_tokens,
                    ..CommentaryContextOptions::default()
                },
            );

            Ok((
                prompt,
                target.content_hash.clone(),
                crate::pipeline::explain::docs::CommentaryDocSymbolMetadata {
                    qualified_name: target.qualified_name(),
                    source_path: target.file.path.clone(),
                },
            ))
        })?;

        match generator.generate(id, &prompt)? {
            Some(mut entry) => {
                // Fill in the source content hash so the entry is immediately Fresh.
                entry.provenance.source_content_hash = content_hash.clone();
                let text = entry.text.clone();
                overlay.lock().insert_commentary(entry.clone())?;
                let synrepo_dir = Config::synrepo_dir(repo_root);
                if let Some(path) = crate::pipeline::explain::docs::upsert_commentary_doc(
                    &synrepo_dir,
                    id,
                    &entry,
                    FreshnessState::Fresh,
                    &doc_metadata,
                )? {
                    crate::pipeline::explain::docs::sync_commentary_index(&synrepo_dir, &[path])?;
                }
                Ok(Some(text))
            }
            None => Ok(None),
        }
    }
}

impl CardCompiler for GraphCardCompiler {
    fn symbol_card(&self, id: SymbolNodeId, budget: Budget) -> crate::Result<SymbolCard> {
        let result = self.with_reader(|graph| {
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
        });
        // Force HEAD probe on the next compile to handle commits between card builds.
        self.git_cache.on_compile_cycle_end();
        result
    }

    fn file_card(&self, id: FileNodeId, budget: Budget) -> crate::Result<FileCard> {
        let result = self.with_reader(|graph| file::file_card(self, graph, id, budget));
        self.git_cache.on_compile_cycle_end();
        result
    }

    fn entry_point_card(
        &self,
        scope: Option<&str>,
        budget: Budget,
    ) -> crate::Result<EntryPointCard> {
        let result =
            self.with_reader(|graph| entry_point::entry_point_card_impl(graph, scope, budget));
        self.git_cache.on_compile_cycle_end();
        result
    }

    fn module_card(&self, path: &str, budget: Budget) -> crate::Result<ModuleCard> {
        let result = self.with_reader(|graph| module::module_card_impl(graph, path, budget));
        self.git_cache.on_compile_cycle_end();
        result
    }

    fn public_api_card(&self, path: &str, budget: Budget) -> crate::Result<PublicAPICard> {
        let result =
            self.with_reader(|graph| public_api::public_api_card_impl(self, graph, path, budget));
        self.git_cache.on_compile_cycle_end();
        result
    }

    fn call_path_card(
        &self,
        target: SymbolNodeId,
        budget: Budget,
    ) -> crate::Result<super::CallPathCard> {
        let result =
            self.with_reader(|graph| call_path::call_path_card_impl(graph, target, budget));
        self.git_cache.on_compile_cycle_end();
        result
    }

    fn test_surface_card(
        &self,
        scope: &str,
        budget: Budget,
    ) -> crate::Result<super::TestSurfaceCard> {
        let result =
            self.with_reader(|graph| test_surface::test_surface_card_impl(graph, scope, budget));
        self.git_cache.on_compile_cycle_end();
        result
    }

    fn resolve_target(&self, target: &str) -> crate::Result<Option<NodeId>> {
        let result = self.with_reader(|graph| resolve::resolve_target(graph, target));
        self.git_cache.on_compile_cycle_end();
        result
    }

    fn change_risk_card(&self, target: NodeId, budget: Budget) -> crate::Result<ChangeRiskCard> {
        let result =
            self.with_reader(|graph| change_risk::change_risk_card(self, graph, target, budget));
        self.git_cache.on_compile_cycle_end();
        result
    }
}
