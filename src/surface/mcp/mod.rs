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
use crate::overlay::OverlayStore;
use crate::surface::card::compiler::GraphCardCompiler;

pub mod audit;
pub mod cards;
pub mod helpers;
pub mod primitives;
pub mod search;

mod findings;

/// Shared read-only state held across all MCP tool invocations.
pub struct SynrepoState {
    /// The card compiler, which owns the graph store handle.
    pub compiler: GraphCardCompiler,
    /// Runtime configuration loaded from `.synrepo/config.toml`.
    pub config: Config,
    /// Absolute path to the repository root.
    pub repo_root: PathBuf,
    /// Overlay store handle shared with the compiler.
    pub overlay: Arc<Mutex<dyn OverlayStore>>,
}

const _: () = {
    fn _assert_send_sync<T: Send + Sync>() {}
    fn _check() {
        _assert_send_sync::<SynrepoState>();
    }
};
