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
use crate::structure::graph::snapshot;
use crate::surface::card::compiler::GraphCardCompiler;

pub mod audit;
pub mod card_accounting;
pub mod cards;
pub mod context_pack;
pub mod docs;
pub mod helpers;
pub mod notes;
pub mod primitives;
pub mod search;

mod findings;

#[cfg(test)]
mod snapshot_tests;

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
    /// Create a fresh, request-local compiler backed by SQLite.
    ///
    /// The caller is responsible for holding the handle for the duration of
    /// a single tool request and then dropping it to release the SQLite
    /// connections and their associated snapshots.
    pub fn create_sqlite_compiler(&self) -> crate::Result<GraphCardCompiler> {
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

    /// Create a fresh, request-local compiler for read-only MCP tools.
    pub fn create_read_compiler(&self) -> crate::Result<GraphCardCompiler> {
        use crate::store::overlay::SqliteOverlayStore;

        if graph_snapshot_disabled() {
            return self.create_sqlite_compiler();
        }

        let graph = snapshot::current();
        if graph.snapshot_epoch == 0 {
            return self.create_sqlite_compiler();
        }

        let synrepo_dir = Config::synrepo_dir(&self.repo_root);
        let overlay_dir = synrepo_dir.join("overlay");
        let overlay = SqliteOverlayStore::open_existing(&overlay_dir).ok();

        let mut compiler =
            GraphCardCompiler::new_with_snapshot(graph, Some(self.repo_root.clone()))
                .with_config(self.config.clone());

        if let Some(overlay) = overlay {
            compiler = compiler.with_overlay(Some(Arc::new(Mutex::new(overlay))));
        }

        Ok(compiler)
    }
}

pub(crate) fn graph_snapshot_disabled() -> bool {
    std::env::var("SYNREPO_DISABLE_GRAPH_SNAPSHOT")
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

const _: () = {
    fn _assert_send_sync<T: Send + Sync>() {}
    fn _check() {
        _assert_send_sync::<SynrepoState>();
    }
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, sync::Arc, thread};

    // Regression guard: a shared `SqliteGraphStore` across threads aliases
    // the per-compiler re-entrant snapshot counter and surfaces as
    // "transaction within a transaction" errors or JSON `error` payloads.
    #[test]
    fn state_supports_concurrent_tool_calls() {
        let home = tempfile::tempdir().unwrap();
        let _home_guard = crate::config::test_home::HomeEnvGuard::redirect_to(home.path());
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        fs::create_dir_all(repo.join("src")).unwrap();
        fs::write(
            repo.join("src/lib.rs"),
            "pub fn first() {}\npub fn second() {}\n",
        )
        .unwrap();
        crate::bootstrap::bootstrap(repo, None, false).unwrap();

        let state = Arc::new(SynrepoState {
            config: Config::load(repo).unwrap(),
            repo_root: repo.to_path_buf(),
        });

        let mut handles = Vec::new();
        for _ in 0..8 {
            let state = Arc::clone(&state);
            handles.push(thread::spawn(move || {
                let out = super::cards::handle_entrypoints(&state, None, "tiny".to_string(), None);
                let val: serde_json::Value =
                    serde_json::from_str(&out).expect("handler returned valid json");
                assert!(
                    val.get("error").is_none(),
                    "concurrent handler returned error: {}",
                    out
                );
                out
            }));
        }

        let outputs: Vec<String> = handles.into_iter().map(|h| h.join().unwrap()).collect();

        let first = &outputs[0];
        for (i, out) in outputs.iter().enumerate().skip(1) {
            assert_eq!(
                first, out,
                "concurrent handler {i} returned a different payload",
            );
        }
    }
}
