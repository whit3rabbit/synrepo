//! The `GraphStore` trait: canonical graph persistence interface.

use std::collections::HashMap;

use crate::core::ids::{ConceptNodeId, EdgeId, FileNodeId, NodeId, SymbolNodeId};
use crate::structure::drift::StructuralFingerprint;

use super::edge::{Edge, EdgeKind};
use super::node::{ConceptNode, FileNode, SymbolKind, SymbolNode, Visibility};

/// Read-only access to the canonical graph.
pub trait GraphReader: Send + Sync {
    /// Look up a file node by its stable ID.
    fn get_file(&self, id: FileNodeId) -> crate::Result<Option<FileNode>>;

    /// Look up a symbol node by its stable ID.
    fn get_symbol(&self, id: SymbolNodeId) -> crate::Result<Option<SymbolNode>>;

    /// Look up a concept node by its stable ID.
    fn get_concept(&self, id: ConceptNodeId) -> crate::Result<Option<ConceptNode>>;

    /// Find the file node currently associated with a given path.
    fn file_by_path(&self, path: &str) -> crate::Result<Option<FileNode>>;

    /// Find the file node currently associated with a path inside a specific root.
    fn file_by_root_path(&self, root_id: &str, path: &str) -> crate::Result<Option<FileNode>> {
        Ok(self
            .file_by_path(path)?
            .filter(|file| file.root_id == root_id))
    }

    /// All outbound edges from a node, optionally filtered by kind.
    fn outbound(&self, from: NodeId, kind: Option<EdgeKind>) -> crate::Result<Vec<Edge>>;

    /// All inbound edges to a node, optionally filtered by kind.
    fn inbound(&self, to: NodeId, kind: Option<EdgeKind>) -> crate::Result<Vec<Edge>>;

    /// Return all file paths currently in the graph with their stable node IDs.
    fn all_file_paths(&self) -> crate::Result<Vec<(String, FileNodeId)>>;

    /// Return all concept paths currently in the graph with their stable node IDs.
    fn all_concept_paths(&self) -> crate::Result<Vec<(String, ConceptNodeId)>>;

    /// Return `(id, file_id, qualified_name)` tuples for all symbol nodes.
    fn all_symbol_names(&self) -> crate::Result<Vec<(SymbolNodeId, FileNodeId, String)>>;

    /// Return `(id, file_id, qualified_name, kind_label, body_hash)` tuples for
    /// all symbol nodes in a single batch query.
    #[allow(clippy::type_complexity)]
    fn all_symbols_summary(
        &self,
    ) -> crate::Result<Vec<(SymbolNodeId, FileNodeId, String, String, String)>> {
        let names = self.all_symbol_names()?;
        let mut out = Vec::with_capacity(names.len());
        for (sym_id, file_id, qname) in names {
            if let Ok(Some(sym)) = self.get_symbol(sym_id) {
                out.push((
                    sym_id,
                    file_id,
                    qname,
                    sym.kind.as_str().to_string(),
                    sym.body_hash,
                ));
            }
        }
        Ok(out)
    }

    /// Return `(id, file_id, qualified_name, kind, visibility, body_hash)` tuples for all
    /// active symbol nodes, used by stage-4 call-scope narrowing.
    #[allow(clippy::type_complexity)]
    fn all_symbols_for_resolution(
        &self,
    ) -> crate::Result<
        Vec<(
            SymbolNodeId,
            FileNodeId,
            String,
            SymbolKind,
            Visibility,
            String,
        )>,
    > {
        let names = self.all_symbol_names()?;
        let mut out = Vec::with_capacity(names.len());
        for (sym_id, file_id, qname) in names {
            if let Ok(Some(sym)) = self.get_symbol(sym_id) {
                out.push((
                    sym_id,
                    file_id,
                    qname,
                    sym.kind,
                    sym.visibility,
                    sym.body_hash,
                ));
            }
        }
        Ok(out)
    }

    /// Return all edges in the graph regardless of source node type.
    fn all_edges(&self) -> crate::Result<Vec<Edge>> {
        let mut edges = Vec::new();
        for (_, file_id) in self.all_file_paths()? {
            edges.extend(self.outbound(NodeId::File(file_id), None)?);
        }
        Ok(edges)
    }

    /// Return all active (non-retired) symbols owned by a file.
    fn symbols_for_file(&self, _file_id: FileNodeId) -> crate::Result<Vec<SymbolNode>> {
        Ok(Vec::new())
    }

    /// Return all active (non-retired) edges owned by a file.
    fn edges_owned_by(&self, _file_id: FileNodeId) -> crate::Result<Vec<Edge>> {
        Ok(Vec::new())
    }

    /// Return all active (non-retired) edges in the graph.
    fn active_edges(&self) -> crate::Result<Vec<Edge>> {
        self.all_edges()
    }

    /// Return the count of edges matching `kind`.
    fn count_edges_by_kind(&self, kind: EdgeKind) -> crate::Result<usize> {
        Ok(self
            .all_edges()?
            .into_iter()
            .filter(|edge| edge.kind == kind)
            .count())
    }

    /// Return all `ConceptNode`s that have an outgoing `Governs` edge to `node_id`.
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

/// Trait for the canonical graph store.
///
/// Phase 1 implementation is sqlite-backed; see [`crate::store`].
/// Other backends (in-memory for tests, petgraph for hot queries) can
/// implement this trait without changes to callers.
pub trait GraphStore: GraphReader {
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
        Err(crate::Error::Other(anyhow::anyhow!(
            "delete_edges_by_kind is not implemented for this GraphStore backend"
        )))
    }

    /// Delete a node and all incident edges. Used when a file disappears
    /// and the identity cascade cannot find a new home for it.
    fn delete_node(&mut self, id: NodeId) -> crate::Result<()>;

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

    /// Return the latest revision string stored in the `edge_drift` table, or
    /// `None` when the table is empty or the store does not support drift.
    ///
    /// Default returns `None` so in-memory/test stores need no implementation.
    fn latest_drift_revision(&self) -> crate::Result<Option<String>> {
        Ok(None)
    }

    /// Batch-write drift scores for edges in the given revision.
    /// Default no-op so in-memory/test stores need no implementation.
    fn write_drift_scores(
        &mut self,
        _scores: &[(EdgeId, f32)],
        _revision: &str,
    ) -> crate::Result<()> {
        Ok(())
    }

    /// Read all drift scores for a given revision.
    /// Default returns empty so in-memory/test stores need no implementation.
    fn read_drift_scores(&self, _revision: &str) -> crate::Result<Vec<(EdgeId, f32)>> {
        Ok(Vec::new())
    }

    /// Delete drift scores older than the given revision.
    /// Default no-op so in-memory/test stores need no implementation.
    fn truncate_drift_scores(&self, _older_than_revision: &str) -> crate::Result<usize> {
        Ok(0)
    }

    /// Return whether any drift rows exist at all (across all revisions).
    /// Used by the repair loop to distinguish "never assessed" from "assessed and
    /// healthy."
    fn has_any_drift_scores(&self) -> crate::Result<bool> {
        Ok(false)
    }

    /// Return the latest revision stored in the `file_fingerprints` table, or
    /// `None` when the table is empty. Used by drift scoring to find the prior
    /// revision's fingerprints for comparison.
    fn latest_fingerprint_revision(&self) -> crate::Result<Option<String>> {
        Ok(None)
    }

    /// Batch-write structural fingerprints for files at a given revision.
    /// Default no-op so in-memory/test stores need no implementation.
    fn write_fingerprints(
        &mut self,
        _fingerprints: &[(FileNodeId, StructuralFingerprint)],
        _revision: &str,
    ) -> crate::Result<()> {
        Ok(())
    }

    /// Read all fingerprints for a given revision.
    /// Default returns empty so in-memory/test stores need no implementation.
    fn read_fingerprints(
        &self,
        _revision: &str,
    ) -> crate::Result<HashMap<FileNodeId, StructuralFingerprint>> {
        Ok(HashMap::new())
    }

    /// Delete fingerprints older than the given revision.
    /// Default no-op so in-memory/test stores need no implementation.
    fn truncate_fingerprints(&self, _older_than_revision: &str) -> crate::Result<usize> {
        Ok(0)
    }

    // -- Observation lifecycle (graph-lifecycle-v1) --------------------------

    /// Allocate the next compile revision and return its id.
    /// Default returns 0 so in-memory/test stores need no implementation.
    fn next_compile_revision(&mut self) -> crate::Result<u64> {
        Ok(0)
    }

    /// Mark a symbol as retired at the given compile revision.
    fn retire_symbol(&mut self, _id: SymbolNodeId, _revision: u64) -> crate::Result<()> {
        Ok(())
    }

    /// Mark an edge as retired at the given compile revision.
    fn retire_edge(&mut self, _id: EdgeId, _revision: u64) -> crate::Result<()> {
        Ok(())
    }

    /// Mark many symbols retired in one batch. Default impl loops over
    /// `retire_symbol` so non-SQLite backends still work; the SQLite impl
    /// overrides with a single chunked `UPDATE ... WHERE id IN (...)`.
    fn retire_symbols_bulk(&mut self, ids: &[SymbolNodeId], revision: u64) -> crate::Result<()> {
        for id in ids {
            self.retire_symbol(*id, revision)?;
        }
        Ok(())
    }

    /// Mark many edges retired in one batch. See `retire_symbols_bulk`.
    fn retire_edges_bulk(&mut self, ids: &[EdgeId], revision: u64) -> crate::Result<()> {
        for id in ids {
            self.retire_edge(*id, revision)?;
        }
        Ok(())
    }

    /// Clear retirement on a symbol and set its last_observed_rev.
    fn unretire_symbol(&mut self, _id: SymbolNodeId, _revision: u64) -> crate::Result<()> {
        Ok(())
    }

    /// Clear retirement on an edge and set its last_observed_rev.
    fn unretire_edge(&mut self, _id: EdgeId, _revision: u64) -> crate::Result<()> {
        Ok(())
    }

    /// Physically delete retired nodes/edges older than `older_than_rev`.
    /// Returns counts of removed rows. Default no-op.
    fn compact_retired(&mut self, _older_than_rev: u64) -> crate::Result<CompactionSummary> {
        Ok(CompactionSummary::default())
    }
}

/// Summary of a compaction pass over retired observations.
#[derive(Clone, Debug, Default)]
pub struct CompactionSummary {
    /// Number of retired symbols physically deleted.
    pub symbols_removed: usize,
    /// Number of retired edges physically deleted.
    pub edges_removed: usize,
    /// Number of old compile_revisions rows removed.
    pub revisions_removed: usize,
}

/// Run `f` against `graph` with a read snapshot held for its duration.
///
/// Pairs `begin_read_snapshot` and `end_read_snapshot` structurally: the
/// snapshot is always ended, even if `f` returns `Err`, so callers cannot
/// accidentally leak an open transaction on the error path. An end-failure
/// is swallowed on purpose; the snapshot does not outlive this stack frame,
/// and surfacing an end-failure would mask the caller's original error.
struct SnapshotGuard<'a>(&'a dyn GraphStore);

impl Drop for SnapshotGuard<'_> {
    fn drop(&mut self) {
        if let Err(err) = self.0.end_read_snapshot() {
            // Why: warn level so a stuck COMMIT (busy_timeout exhaustion)
            // is visible in default log output, not buried at debug.
            tracing::warn!(error = %err, "graph end_read_snapshot failed; ignoring");
        }
    }
}

/// Run `f` against `graph` with a read snapshot held for its duration.
///
/// Pairs `begin_read_snapshot` and `end_read_snapshot` structurally: the
/// snapshot is always ended, even if `f` panics or returns `Err`, so callers cannot
/// accidentally leak an open transaction on the error path. An end-failure
/// is swallowed on purpose; the snapshot does not outlive this stack frame,
/// and surfacing an end-failure would mask the caller's original error.
pub fn with_graph_read_snapshot<F, R>(graph: &dyn GraphStore, f: F) -> crate::Result<R>
where
    F: FnOnce(&dyn GraphReader) -> crate::Result<R>,
{
    graph.begin_read_snapshot()?;
    let _guard = SnapshotGuard(graph);
    f(graph)
}
