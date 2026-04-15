//! The `GraphStore` trait: canonical graph persistence interface.

use crate::core::ids::{ConceptNodeId, EdgeId, FileNodeId, NodeId, SymbolNodeId};

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

    /// Delete a single edge by id. Used by compensation paths that must
    /// unwind a speculative `insert_edge` when a paired cross-store write
    /// fails. Default impl returns an error so stores that do not wire it
    /// up surface a clear diagnostic instead of silently succeeding.
    fn delete_edge(&mut self, _edge_id: EdgeId) -> crate::Result<()> {
        Err(crate::Error::Other(anyhow::anyhow!(
            "delete_edge is not implemented for this GraphStore backend"
        )))
    }

    /// Delete all edges of a given kind. Returns the number of deleted edges.
    /// Used for full re-emit strategies where a category of edges is rebuilt
    /// from scratch each reconcile (e.g. CoChangesWith).
    fn delete_edges_by_kind(&mut self, _kind: EdgeKind) -> crate::Result<usize> {
        Ok(0)
    }

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

    /// Open a read snapshot on this store so that every subsequent read
    /// through this handle observes a single committed epoch until
    /// [`GraphStore::end_read_snapshot`] is called.
    ///
    /// Why this exists: a card compile or traversal issues many read
    /// queries (file, symbols, inbound/outbound edges, concepts). Without a
    /// snapshot, a writer commit between queries leaves the caller looking
    /// at two different epochs at once, which is how agents end up
    /// hallucinating about structure. With a snapshot, the reader sees one
    /// epoch for the entire operation; a concurrent writer still commits,
    /// but its result is invisible to this reader until the snapshot ends.
    ///
    /// Contract:
    /// - Must be paired with `end_read_snapshot`. Prefer the
    ///   `with_graph_read_snapshot` helper so the pairing is structural.
    /// - Nesting is safe: implementations are re-entrant via a depth
    ///   counter, so an outer wrap (e.g. an MCP handler) may compose
    ///   cleanly with an inner wrap (e.g. `GraphCardCompiler::symbol_card`)
    ///   without tripping the backing store. All nested levels share the
    ///   outermost committed epoch; inner begins do NOT observe writes
    ///   that the outer begin had not yet seen.
    /// - Writer-side methods (`begin`/`commit`/`rollback`) are a separate
    ///   lane and must not interleave with a read snapshot on the same
    ///   handle.
    ///
    /// Default no-op so in-memory/test stores need no implementation.
    fn begin_read_snapshot(&self) -> crate::Result<()> {
        Ok(())
    }

    /// Close a read snapshot previously opened by `begin_read_snapshot`.
    ///
    /// Implementations should tolerate being called when no snapshot is
    /// active (returns `Ok(())`), so the "always end" idiom in
    /// `with_graph_read_snapshot` stays safe on the error path.
    fn end_read_snapshot(&self) -> crate::Result<()> {
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

    /// Return all `ConceptNode`s that have an outgoing `Governs` edge to `node_id`.
    ///
    /// This is a default method backed by `inbound` + `get_concept` so all
    /// `GraphStore` implementations inherit it automatically.
    fn find_governing_concepts(&self, node_id: NodeId) -> crate::Result<Vec<ConceptNode>> {
        let edges = self.inbound(node_id, Some(EdgeKind::Governs))?;
        let mut concepts = Vec::new();
        for edge in edges {
            if let NodeId::Concept(concept_id) = edge.from {
                if let Some(concept) = self.get_concept(concept_id)? {
                    concepts.push(concept);
                }
            }
        }
        Ok(concepts)
    }
}

/// Run `f` against `graph` with a read snapshot held for its duration.
///
/// Pairs `begin_read_snapshot` and `end_read_snapshot` structurally: the
/// snapshot is always ended, even if `f` returns `Err`, so callers cannot
/// accidentally leak an open transaction on the error path. An end-failure
/// is swallowed on purpose; the snapshot does not outlive this stack frame,
/// and surfacing an end-failure would mask the caller's original error.
pub fn with_graph_read_snapshot<F, R>(graph: &dyn GraphStore, f: F) -> crate::Result<R>
where
    F: FnOnce(&dyn GraphStore) -> crate::Result<R>,
{
    graph.begin_read_snapshot()?;
    let result = f(graph);
    if let Err(err) = graph.end_read_snapshot() {
        tracing::debug!(error = %err, "end_read_snapshot failed; ignoring");
    }
    result
}
