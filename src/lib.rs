//! synrepo — a context compiler for AI coding agents
//!
//! Architecture (four layers, bottom to top):
//!
//! 1. **Substrate layer** — file discovery, classification, and [`syntext`] n-gram index.
//!    - [`substrate::discover`] walks the filesystem
//!    - [`substrate::classify`] maps files to content tiers
//!    - [`substrate::index`] builds and queries the lexical index
//! 2. **Structure layer** — the canonical graph of observed facts:
//!    - [`parse`] runs tree-sitter and the markdown parser
//!    - [`graph`] is the sqlite-backed graph store
//!    - [`identity`] handles AST-based rename detection
//!    - [`drift`] scores per-edge Jaccard distance over persisted structural
//!      fingerprints (stage 7 — implemented, sidecar `edge_drift` / `file_fingerprints` tables).
//!      Stage 8 (ArcSwap commit) is still a TODO.
//! 3. **Overlay layer** — LLM-authored content, physically separate from the graph.
//!    Phase 4+ only; module exists to enforce the architectural boundary. See [`overlay`].
//! 4. **Surface layer** — CLI (`src/bin/cli.rs`), MCP server (`synrepo mcp` subcommand),
//!    and skill bundle (`skill/SKILL.md`). MCP tool handlers live in [`surface::mcp`].
//!
//! **Bootstrap** (`bootstrap`) — first-run UX, mode detection, health checks.
//!    [`bootstrap::bootstrap`] is the main entry point for `synrepo init`.
//!
//! The canonical/overlay separation is **structural**, not merely labeled:
//! graph data lives in `graph/*.db`, overlay data lives in `overlay/*.db`,
//! and synthesis queries filter at the retrieval layer so the synthesis
//! pipeline never reads its own previous output.
//!
//! See `docs/FOUNDATION.md` and `docs/FOUNDATION-SPEC.md` for design documentation.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(clippy::all)]

#[cfg(test)]
mod docs_drift;

pub mod bootstrap;
pub mod config;
pub mod core;
pub mod error;
pub mod overlay;
pub mod pipeline;
pub mod store;
pub mod structure;
pub mod substrate;
pub mod surface;
#[doc(hidden)]
pub mod test_support;
pub mod tui;
pub mod util;

pub use crate::core::ids::{ConceptNodeId, EdgeId, FileNodeId, NodeId, SymbolNodeId};
pub use error::{Error, Result};
