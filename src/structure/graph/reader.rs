//! Read-only graph access trait.

use crate::core::ids::{ConceptNodeId, EdgeId, FileNodeId, NodeId, SymbolNodeId};

use super::edge::{Edge, EdgeKind};
use super::node::{ConceptNode, FileNode, SymbolKind, SymbolNode, Visibility};

/// Read-only access to the canonical graph.
pub trait GraphReader: Send + Sync {
    /// Look up a file node by its stable ID.
    fn get_file(&self, id: FileNodeId) -> crate::Result<Option<FileNode>>;

    /// Look up multiple file nodes by stable ID. Missing IDs are omitted.
    fn get_files(&self, ids: &[FileNodeId]) -> crate::Result<Vec<FileNode>> {
        ids.iter()
            .filter_map(|id| self.get_file(*id).transpose())
            .collect()
    }

    /// Look up a symbol node by its stable ID.
    fn get_symbol(&self, id: SymbolNodeId) -> crate::Result<Option<SymbolNode>>;

    /// Look up multiple symbol nodes by stable ID. Missing IDs are omitted.
    fn get_symbols(&self, ids: &[SymbolNodeId]) -> crate::Result<Vec<SymbolNode>> {
        ids.iter()
            .filter_map(|id| self.get_symbol(*id).transpose())
            .collect()
    }

    /// Look up a concept node by its stable ID.
    fn get_concept(&self, id: ConceptNodeId) -> crate::Result<Option<ConceptNode>>;

    /// Look up multiple concept nodes by stable ID. Missing IDs are omitted.
    fn get_concepts(&self, ids: &[ConceptNodeId]) -> crate::Result<Vec<ConceptNode>> {
        ids.iter()
            .filter_map(|id| self.get_concept(*id).transpose())
            .collect()
    }

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

    /// Latest revision in `edge_drift`, or `None` when empty / unsupported.
    fn latest_drift_revision(&self) -> crate::Result<Option<String>> {
        Ok(None)
    }

    /// Drift scores for a revision. Empty default for in-memory/test stores.
    fn read_drift_scores(&self, _revision: &str) -> crate::Result<Vec<(EdgeId, f32)>> {
        Ok(Vec::new())
    }
}
