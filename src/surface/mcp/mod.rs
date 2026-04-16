//! MCP tool handlers for the synrepo MCP server.
//!
//! This module provides the tool implementation functions used by the
//! `synrepo mcp` subcommand. Each sub-module owns one tool category:
//! card-returning tools, search/routing tools, audit tools, and graph
//! primitives.
//!
//! `SynrepoState` is the shared read-only state held across all tool
//! invocations. It is constructed by the binary-side MCP command and
//! passed to every handler.
#![allow(missing_docs)]

use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;

use crate::config::Config;
use crate::surface::card::compiler::GraphCardCompiler;

pub mod audit;
pub mod cards;
pub mod helpers;
pub mod primitives;
pub mod search;

mod findings;

/// Shared read-only state held across all MCP tool invocations.
///
/// In previous versions, this held a single shared `GraphCardCompiler`.
/// However, `SqliteGraphStore` is not safe for concurrent requests sharing
/// one handle because of its re-entrant `snapshot_depth` counter.
///
/// We now store the paths and configuration needed to instantiate a fresh,
/// request-local compiler for every tool invocation. This allows multiple
/// concurrent tool requests to hold independent read snapshots in WAL mode,
/// preventing "snapshot piggybacking" and unbounded WAL growth.
pub struct SynrepoState {
    /// Runtime configuration loaded from `.synrepo/config.toml`.
    pub config: Config,
    /// Absolute path to the repository root.
    pub repo_root: PathBuf,
}

impl SynrepoState {
    /// Create a fresh, request-local compiler.
    ///
    /// The caller is responsible for holding the handle for the duration of
    /// a single tool request and then dropping it to release the SQLite
    /// connections and their associated snapshots.
    pub fn create_compiler(
        &self,
    ) -> crate::Result<crate::surface::card::compiler::GraphCardCompiler> {
        use crate::store::overlay::SqliteOverlayStore;
        use crate::store::sqlite::SqliteGraphStore;

        let synrepo_dir = Config::synrepo_dir(&self.repo_root);
        let graph_dir = synrepo_dir.join("graph");
        let overlay_dir = synrepo_dir.join("overlay");

        let graph = SqliteGraphStore::open_existing(&graph_dir)?;
        let overlay = SqliteOverlayStore::open_existing(&overlay_dir).ok();

        let mut compiler = GraphCardCompiler::new(Box::new(graph), Some(self.repo_root.clone()))
            .with_config(self.config.clone());

        if let Some(overlay) = overlay {
            compiler = compiler.with_overlay(Some(Arc::new(Mutex::new(overlay))));
        }

        Ok(compiler)
    }
}

const _: () = {
    fn _assert_send_sync<T: Send + Sync>() {}
    fn _check() {
        _assert_send_sync::<SynrepoState>();
    }
};
