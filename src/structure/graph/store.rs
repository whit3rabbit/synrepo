//! The `GraphStore` trait: canonical graph persistence interface.

use crate::core::ids::{ConceptNodeId, FileNodeId, NodeId, SymbolNodeId};

use super::edge::{Edge, EdgeKind};
use super::node::{ConceptNode, FileNode, SymbolNode};

/// Trait for the canonical graph store.
///
/// Phase 1 implementation is sqlite-backed; see [`crate::store`].
/// Other backends (in-memory for tests, petgraph for hot queries) can
/// implement this trait without changes to callers.
pub trait GraphStore: Send + Sync {
    /// Insert or update a file node.
    fn upsert_file(&mut self, node: FileNode) -> crate::Result<()>;

    /// Insert or update a symbol node.
    fn upsert_symbol(&mut self, node: SymbolNode) -> crate::Result<()>;

    /// Insert or update a concept node.
    fn upsert_concept(&mut self, node: ConceptNode) -> crate::Result<()>;

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

    /// Look up a concept node by its stable ID.
    fn get_concept(&self, id: ConceptNodeId) -> crate::Result<Option<ConceptNode>>;

    /// Find the file node currently associated with a given path.
    fn file_by_path(&self, path: &str) -> crate::Result<Option<FileNode>>;

    /// All outbound edges from a node, optionally filtered by kind.
    fn outbound(&self, from: NodeId, kind: Option<EdgeKind>) -> crate::Result<Vec<Edge>>;

    /// All inbound edges to a node, optionally filtered by kind.
    fn inbound(&self, to: NodeId, kind: Option<EdgeKind>) -> crate::Result<Vec<Edge>>;

    /// Begin a write batch. Called before the first graph interaction in each
    /// structural compile cycle. No-op for in-memory or test stores.
    fn begin(&mut self) -> crate::Result<()> {
        Ok(())
    }

    /// Commit any pending writes. Called at the end of each structural
    /// compile cycle to publish atomic snapshots.
    fn commit(&mut self) -> crate::Result<()>;

    /// Roll back any pending transaction. Called on error paths to prevent
    /// leaving SQLite in an open-transaction state. No-op for in-memory/test stores.
    fn rollback(&mut self) -> crate::Result<()> {
        Ok(())
    }

    /// Return all file paths currently in the graph with their stable node IDs.
    /// Used by the structural compile to detect stale file facts.
    fn all_file_paths(&self) -> crate::Result<Vec<(String, FileNodeId)>>;

    /// Return all concept paths currently in the graph with their stable node IDs.
    /// Used by the structural compile to detect stale concept facts.
    fn all_concept_paths(&self) -> crate::Result<Vec<(String, ConceptNodeId)>>;

    /// Return (id, file_id, qualified_name) tuples for all symbol nodes.
    ///
    /// Used by the stage-4 name-resolution pass to build the global symbol
    /// index without loading full JSON blobs.
    fn all_symbol_names(&self) -> crate::Result<Vec<(SymbolNodeId, FileNodeId, String)>>;
}
