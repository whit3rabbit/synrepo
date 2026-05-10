//! synrepo — a context compiler for AI coding agents
//!
//! Architecture (five product layers plus runtime/orchestration modules):
//!
//! 0. **Core layer** — shared IDs, provenance, source-language metadata, path
//!    safety, and project-layout detection.
//! 1. **Substrate layer** — file discovery, classification, and [`syntext`] n-gram index.
//!    - [`mod@substrate::discover`] walks the filesystem
//!    - [`mod@substrate::classify`] maps files to content tiers
//!    - [`substrate::index`] builds and queries the lexical index
//! 2. **Structure layer** — the canonical graph of observed facts:
//!    - [`structure::parse`] runs tree-sitter and the markdown parser
//!    - [`structure::graph`] is the sqlite-backed graph store
//!    - [`structure::identity`] handles AST-based rename detection
//!    - [`structure::drift`] scores per-edge Jaccard distance over persisted structural
//!      fingerprints (stage 7 — implemented, sidecar `edge_drift` / `file_fingerprints` tables).
//!    - [`structure::graph::snapshot`] publishes the immutable in-memory `Graph` after each successful compile.
//! 3. **Overlay layer** — machine-authored and advisory content, physically
//!    separate from the graph. See [`overlay`].
//! 4. **Surface layer** — CLI (`src/bin/cli.rs`), MCP server (`synrepo mcp` subcommand),
//!    and skill bundle (`skill/SKILL.md`). MCP tool handlers live in [`surface::mcp`].
//!
//! **Bootstrap** (`bootstrap`) — first-run UX, mode detection, health checks.
//!    [`bootstrap::bootstrap`] is the main entry point for `synrepo init`.
//!
//! The canonical/overlay separation is **structural**, not merely labeled:
//! graph data lives in `graph/*.db`, overlay data lives in `overlay/*.db`,
//! and explain queries filter at the retrieval layer so the explain
//! pipeline never reads its own previous output.
//!
//! See `docs/ARCHITECTURE.md` and `docs/FOUNDATION.md` for design documentation.

// Crate-wide ban on unsafe code. `deny` (not `forbid`) so we can scope
// narrow, audited exceptions — currently only the Windows `is_process_alive`
// path in `pipeline::writer::helpers`, which calls `OpenProcess` /
// `GetExitCodeProcess` / `CloseHandle`. Every other module stays
// unsafe-free; `deny` fails the build on any accidental reintroduction.
#![deny(unsafe_code)]
#![warn(missing_docs)]
#![warn(clippy::all)]

#[cfg(test)]
mod docs_drift;

pub mod agent_install;
pub mod bootstrap;
pub mod config;
pub mod core;
pub mod error;
pub mod overlay;
pub mod pipeline;
pub mod registry;
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
