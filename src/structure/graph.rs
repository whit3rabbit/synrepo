//! The canonical graph: nodes and edges that were directly observed by
//! parsers, git, or humans. Machine-authored content does not exist in
//! this layer — it lives in [`crate::overlay`].

use serde::{Deserialize, Serialize};

use crate::core::ids::{EdgeId, FileNodeId, NodeId, SymbolNodeId};
use crate::core::provenance::Provenance;

/// Epistemic origin of a graph row.
///
/// The canonical graph only holds `parser_observed`, `human_declared`, and
/// `git_observed` rows. Machine-authored content lives in the overlay and
/// uses [`crate::overlay::OverlayEpistemic`] instead. This enum does not
/// include the machine variants on purpose — the type system enforces the
/// graph/overlay boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Epistemic {
    /// Directly observed by tree-sitter or the markdown parser.
    ParserObserved,
    /// Present in a human-authored source (frontmatter, inline marker, ADR).
    HumanDeclared,
    /// Derived from git history (rename, co-change, ownership, blame).
    GitObserved,
}

/// Kind of a symbol node.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    /// A free function.
    Function,
    /// A method on a class or struct.
    Method,
    /// A class or struct.
    Class,
    /// A trait, interface, or protocol.
    Trait,
    /// A type alias or typedef.
    Type,
    /// A module or namespace.
    Module,
    /// A constant or static.
    Constant,
    /// A top-level exported item (when the language has explicit exports).
    Export,
}

/// Kind of a graph edge.
///
/// Restricted to observed relationships. Edge types the synthesis layer
/// proposes live on [`crate::overlay::OverlayEdgeKind`] instead.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    /// `from` imports `to` (module-level import statement).
    Imports,
    /// `from` calls `to` (function call).
    Calls,
    /// `from` inherits from `to` (class inheritance / trait bound).
    Inherits,
    /// `from` defines `to` (file defines symbol, module defines submodule).
    Defines,
    /// `from` references `to` (generic "uses" relationship, parser-observed).
    References,
    /// A markdown file mentions a code identifier (link parser).
    Mentions,
    /// `from` and `to` co-change in git history without an import edge.
    CoChangesWith,
    /// A human-declared ADR or inline marker declares governance.
    /// Only created from frontmatter or inline `# DECISION:` markers,
    /// never inferred.
    Governs,
    /// Provenance: this file was split from another during a refactor.
    SplitFrom,
    /// Provenance: this file was merged from another during a refactor.
    MergedFrom,
}

/// A file node in the canonical graph.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileNode {
    /// Stable identifier.
    pub id: FileNodeId,
    /// Current path relative to the repo root.
    pub path: String,
    /// Previous paths the file has had, newest first. Appended on rename.
    pub path_history: Vec<String>,
    /// blake3 hash of the current content.
    pub content_hash: String,
    /// File size in bytes.
    pub size_bytes: u64,
    /// Detected language, if supported by a tree-sitter grammar.
    pub language: Option<String>,
    /// Epistemic origin (always `ParserObserved` for files).
    pub epistemic: Epistemic,
    /// Provenance metadata.
    pub provenance: Provenance,
}

/// A symbol node in the canonical graph.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SymbolNode {
    /// Stable identifier derived from `(file_id, qualified_name, kind, body_hash)`.
    pub id: SymbolNodeId,
    /// The file that defines this symbol.
    pub file_id: FileNodeId,
    /// Fully qualified name within the file (e.g. `MyClass::my_method`).
    pub qualified_name: String,
    /// Short display name (just the trailing component).
    pub display_name: String,
    /// Symbol kind.
    pub kind: SymbolKind,
    /// Byte offsets into the file where the symbol body begins and ends.
    pub body_byte_range: (u32, u32),
    /// blake3 hash of the symbol's body bytes.
    pub body_hash: String,
    /// One-line signature (e.g. `pub fn parse_query(input: &str) -> Result<Query, ParseError>`).
    pub signature: Option<String>,
    /// Doc comment text, if any (extracted by the language's `extra.scm`).
    pub doc_comment: Option<String>,
    /// Epistemic origin (always `ParserObserved` for symbols).
    pub epistemic: Epistemic,
    /// Provenance metadata.
    pub provenance: Provenance,
}

/// A graph edge.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Edge {
    /// Stable identifier.
    pub id: EdgeId,
    /// Source node.
    pub from: NodeId,
    /// Target node.
    pub to: NodeId,
    /// Relationship kind.
    pub kind: EdgeKind,
    /// Epistemic origin.
    pub epistemic: Epistemic,
    /// Drift score in [0.0, 1.0]; 0 means fresh, 1 means maximally drifted.
    /// Updated by the structural pipeline on every commit.
    pub drift_score: f32,
    /// Provenance metadata.
    pub provenance: Provenance,
}

/// Trait for the canonical graph store.
///
/// Phase 1 implementation is sqlite-backed; see [`crate::store::sqlite`].
/// Other backends (in-memory for tests, petgraph for hot queries) can
/// implement this trait without changes to callers.
pub trait GraphStore: Send + Sync {
    /// Insert or update a file node.
    fn upsert_file(&mut self, node: FileNode) -> crate::Result<()>;

    /// Insert or update a symbol node.
    fn upsert_symbol(&mut self, node: SymbolNode) -> crate::Result<()>;

    /// Insert an edge. Edges are immutable once committed; to change an
    /// edge, delete it and insert a new one.
    fn insert_edge(&mut self, edge: Edge) -> crate::Result<()>;

    /// Delete a node and all incident edges. Used when a file disappears
    /// and the identity cascade cannot find a new home for it.
    fn delete_node(&mut self, id: NodeId) -> crate::Result<()>;

    /// Look up a file node by its stable ID.
    fn get_file(&self, id: FileNodeId) -> crate::Result<Option<FileNode>>;

    /// Look up a symbol node by its stable ID.
    fn get_symbol(&self, id: SymbolNodeId) -> crate::Result<Option<SymbolNode>>;

    /// Find the file node currently associated with a given path.
    fn file_by_path(&self, path: &str) -> crate::Result<Option<FileNode>>;

    /// All outbound edges from a node, optionally filtered by kind.
    fn outbound(&self, from: NodeId, kind: Option<EdgeKind>) -> crate::Result<Vec<Edge>>;

    /// All inbound edges to a node, optionally filtered by kind.
    fn inbound(&self, to: NodeId, kind: Option<EdgeKind>) -> crate::Result<Vec<Edge>>;

    /// Commit any pending writes. Called at the end of each structural
    /// compile cycle to publish atomic snapshots.
    fn commit(&mut self) -> crate::Result<()>;
}