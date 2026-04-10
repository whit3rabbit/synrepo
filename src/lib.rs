//! synrepo — a context compiler for AI coding agents
//!
//! Architecture (four layers, bottom to top):
//!
//! 1. **Substrate layer** — [`syntext`] n-gram index (not yet wired)
//! 2. **Structure layer** — the canonical graph of observed facts:
//!    - [`discover`] walks the filesystem
//!    - [`parse`] runs tree-sitter and the markdown parser
//!    - [`graph`] is the sqlite-backed graph store
//!    - [`identity`] handles AST-based rename detection
//!    - [`drift`] computes edge drift scores on every commit
//! 3. **Overlay layer** — LLM-authored content, physically separate from the graph.
//!    Not present in phase 0/1; see [`overlay`] stub.
//! 4. **Surface layer** — CLI, MCP server, skill bundle.
//!    Phase 0/1 ships the CLI only; see `src/bin/cli.rs`.
//!
//! The canonical/overlay separation is **structural**, not merely labeled:
//! graph data lives in `graph/*.db`, overlay data lives in `overlay/*.db`,
//! and synthesis queries filter at the retrieval layer so the synthesis
//! pipeline never reads its own previous output.
//!
//! See `synrepo-design-v4.md` for the full design document.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod config;
pub mod core;
pub mod error;
pub mod overlay;
pub mod pipeline;
pub mod store;
pub mod structure;
pub mod surface;
pub mod substrate;

pub use crate::core::ids::{EdgeId, FileNodeId, NodeId, SymbolNodeId};
pub use error::{Error, Result};