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

use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;

use crate::config::Config;
use crate::structure::graph::snapshot;
use crate::surface::card::compiler::GraphCardCompiler;

#[doc(hidden)]
pub mod ask;
mod ask_evidence;
#[doc(hidden)]
pub mod audit;
mod cache;
#[doc(hidden)]
pub mod card_accounting;
#[doc(hidden)]
pub mod card_batch;
#[doc(hidden)]
pub mod card_render;
#[doc(hidden)]
pub mod card_set;
#[doc(hidden)]
pub mod cards;
#[doc(hidden)]
pub mod commentary;
#[doc(hidden)]
pub mod compact;
#[doc(hidden)]
pub mod context_pack;
#[doc(hidden)]
pub mod docs;
#[doc(hidden)]
pub mod edits;
#[doc(hidden)]
pub mod error;
#[doc(hidden)]
pub mod graph;
#[doc(hidden)]
pub mod helpers;
#[doc(hidden)]
pub mod limits;
#[doc(hidden)]
pub mod notes;
#[doc(hidden)]
pub mod primitives;
#[doc(hidden)]
pub mod readiness;
#[doc(hidden)]
pub mod refactor_suggestions;
#[doc(hidden)]
pub mod response_budget;
#[doc(hidden)]
pub mod search;
#[doc(hidden)]
pub mod task_route;

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
    /// Run a closure with a read compiler under MCP resource controls.
    ///
    /// Snapshot-backed compilers are cheap and immutable, so they are created
    /// per call. SQLite-backed fallback compilers are checked out from a small
    /// per-repo pool and returned after the request.
    pub fn with_read_compiler<R>(
        &self,
        f: impl FnOnce(&GraphCardCompiler) -> crate::Result<R>,
    ) -> crate::Result<R> {
        let _permit = cache::acquire_read(&self.repo_root)?;
        if let Some(compiler) = self.snapshot_compiler()? {
            return f(&compiler);
        }

        let compiler = match self.take_pooled_sqlite_compiler().transpose()? {
            Some(compiler) => compiler,
            None => self.create_sqlite_compiler()?,
        };
        let result = f(&compiler);
        self.return_pooled_sqlite_compiler(compiler);
        result
    }

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
        if let Some(compiler) = self.snapshot_compiler()? {
            return Ok(compiler);
        }
        self.create_sqlite_compiler()
    }

    /// Return the overlay open error when a materialized overlay database is unusable.
    pub fn overlay_open_error(&self) -> Option<String> {
        use crate::store::overlay::SqliteOverlayStore;

        let synrepo_dir = Config::synrepo_dir(&self.repo_root);
        let overlay_dir = synrepo_dir.join("overlay");
        let overlay_db = SqliteOverlayStore::db_path(&overlay_dir);
        if !overlay_db.exists() {
            return None;
        }
        SqliteOverlayStore::open_existing(&overlay_dir)
            .err()
            .map(|error| error.to_string())
    }

    /// Require a materialized overlay database, when present, to open cleanly.
    pub fn require_overlay_available(&self) -> anyhow::Result<()> {
        if let Some(error) = self.overlay_open_error() {
            return Err(self::error::McpError::not_initialized(format!(
                "overlay store unavailable: {error}"
            ))
            .into());
        }
        Ok(())
    }

    /// Require an overlay database to exist and open cleanly for overlay reads.
    pub fn require_overlay_materialized(&self) -> anyhow::Result<()> {
        use crate::store::overlay::SqliteOverlayStore;

        let synrepo_dir = Config::synrepo_dir(&self.repo_root);
        let overlay_dir = synrepo_dir.join("overlay");
        let overlay_db = SqliteOverlayStore::db_path(&overlay_dir);
        if !overlay_db.exists() {
            return Err(self::error::McpError::not_initialized(format!(
                "overlay store unavailable: overlay store is not materialized at {}",
                overlay_db.display()
            ))
            .into());
        }
        self.require_overlay_available()
    }

    fn snapshot_compiler(&self) -> crate::Result<Option<GraphCardCompiler>> {
        use crate::store::overlay::SqliteOverlayStore;

        if graph_snapshot_disabled() {
            return Ok(None);
        }

        // Per-repo snapshot lookup. `None` = nobody has bootstrapped this
        // repo in this process yet, so fall back to the on-disk store. The
        // singleton was the source of cross-test contamination: the latest
        // bootstrap's graph leaked into every reader regardless of repo.
        let Some(graph) = snapshot::current(&self.repo_root) else {
            return Ok(None);
        };
        if graph.snapshot_epoch == 0 {
            return Ok(None);
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

        Ok(Some(compiler))
    }

    fn take_pooled_sqlite_compiler(&self) -> Option<crate::Result<GraphCardCompiler>> {
        cache::take_compiler(&self.repo_root)
    }

    fn return_pooled_sqlite_compiler(&self, compiler: GraphCardCompiler) {
        cache::return_compiler(&self.repo_root, compiler);
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

    #[test]
    fn card_reports_overlay_unavailable_when_overlay_missing() {
        let home = tempfile::tempdir().unwrap();
        let _home_guard = crate::config::test_home::HomeEnvGuard::redirect_to(home.path());
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        fs::create_dir_all(repo.join("src")).unwrap();
        fs::write(
            repo.join("src/lib.rs"),
            "pub fn overlay_missing_card() {}\n",
        )
        .unwrap();
        crate::bootstrap::bootstrap(repo, None, false).unwrap();
        let synrepo_dir = Config::synrepo_dir(repo);
        let overlay_dir = synrepo_dir.join("overlay");
        let overlay_db = crate::store::overlay::SqliteOverlayStore::db_path(&overlay_dir);
        drop(crate::store::overlay::SqliteOverlayStore::open(&overlay_dir).unwrap());
        fs::write(&overlay_db, b"not sqlite").unwrap();
        let state = SynrepoState {
            config: Config::load(repo).unwrap(),
            repo_root: repo.to_path_buf(),
        };

        let out = super::cards::handle_card(
            &state,
            "src/lib.rs".to_string(),
            "tiny".to_string(),
            None,
            false,
        );
        let val: serde_json::Value = serde_json::from_str(&out).unwrap();

        assert_eq!(val["source_store"], "graph", "{out}");
        assert_eq!(val["overlay_state"], "unavailable", "{out}");
        assert!(val["overlay_error"]
            .as_str()
            .is_some_and(|error| !error.is_empty()));
    }

    #[test]
    fn overlay_tools_report_overlay_unavailable() {
        let home = tempfile::tempdir().unwrap();
        let _home_guard = crate::config::test_home::HomeEnvGuard::redirect_to(home.path());
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        fs::create_dir_all(repo.join("src")).unwrap();
        fs::write(
            repo.join("src/lib.rs"),
            "pub fn overlay_missing_notes() {}\n",
        )
        .unwrap();
        crate::bootstrap::bootstrap(repo, None, false).unwrap();
        let synrepo_dir = Config::synrepo_dir(repo);
        let overlay_dir = synrepo_dir.join("overlay");
        let overlay_db = crate::store::overlay::SqliteOverlayStore::db_path(&overlay_dir);
        drop(crate::store::overlay::SqliteOverlayStore::open(&overlay_dir).unwrap());
        fs::write(&overlay_db, b"not sqlite").unwrap();
        let state = SynrepoState {
            config: Config::load(repo).unwrap(),
            repo_root: repo.to_path_buf(),
        };

        let out = super::notes::handle_notes(
            &state,
            super::notes::NotesParams {
                repo_root: None,
                target_kind: None,
                target: None,
                include_hidden: false,
                limit: 10,
            },
        );
        let val: serde_json::Value = serde_json::from_str(&out).unwrap();

        assert_eq!(val["ok"], false, "{out}");
        assert_eq!(val["error"]["code"], "NOT_INITIALIZED", "{out}");
        assert!(val["error_message"]
            .as_str()
            .is_some_and(|message| message.contains("overlay store unavailable")));
    }
}
