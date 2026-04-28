//! Mutable in-memory graph store for ephemeral compile runs.

use std::collections::HashMap;
use std::time::SystemTime;

use crate::core::ids::{EdgeId, FileNodeId, NodeId, SymbolNodeId};
use crate::structure::drift::StructuralFingerprint;

use super::{Edge, EdgeKind, Graph, GraphReader, GraphStore, SymbolNode};

/// `GraphStore` implementation backed only by process memory.
#[derive(Clone, Debug, Default)]
pub struct MemGraphStore {
    graph: Graph,
    transaction_backup: Option<Graph>,
    next_revision: u64,
    drift_scores: HashMap<String, Vec<(EdgeId, f32)>>,
    fingerprints: HashMap<String, HashMap<FileNodeId, StructuralFingerprint>>,
}

impl MemGraphStore {
    /// Create an empty in-memory graph store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Materialize the active graph snapshot held by this store.
    pub fn into_graph(self) -> crate::Result<Graph> {
        Graph::from_store(&self)
    }

    fn remove_symbol_indexes(&mut self, symbol: &SymbolNode) {
        if let Some(ids) = self.graph.symbols_by_file.get_mut(&symbol.file_id) {
            ids.retain(|id| *id != symbol.id);
            if ids.is_empty() {
                self.graph.symbols_by_file.remove(&symbol.file_id);
            }
        }
        if let Some(ids) = self
            .graph
            .symbols_by_short_name
            .get_mut(&symbol.display_name)
        {
            ids.retain(|id| *id != symbol.id);
            if ids.is_empty() {
                self.graph
                    .symbols_by_short_name
                    .remove(&symbol.display_name);
            }
        }
    }

    fn index_symbol(&mut self, symbol: &SymbolNode) {
        if symbol.retired_at_rev.is_some() {
            return;
        }
        push_sorted_unique(
            self.graph
                .symbols_by_file
                .entry(symbol.file_id)
                .or_default(),
            symbol.id,
        );
        push_sorted_unique(
            self.graph
                .symbols_by_short_name
                .entry(symbol.display_name.clone())
                .or_default(),
            symbol.id,
        );
    }

    fn remove_edge_indexes(&mut self, edge: &Edge) {
        remove_edge_from_bucket(self.graph.edges_by_from.get_mut(&edge.from), edge.id);
        remove_edge_from_bucket(self.graph.edges_by_to.get_mut(&edge.to), edge.id);
        remove_edge_from_bucket(self.graph.edges_by_kind.get_mut(&edge.kind), edge.id);
    }

    fn index_edge(&mut self, edge: &Edge) {
        if edge.retired_at_rev.is_some() {
            return;
        }
        push_edge_sorted(self.graph.edges_by_from.entry(edge.from).or_default(), edge);
        push_edge_sorted(self.graph.edges_by_to.entry(edge.to).or_default(), edge);
        push_edge_sorted(self.graph.edges_by_kind.entry(edge.kind).or_default(), edge);
    }

    fn remove_incident_edges(&mut self, node_id: NodeId) {
        let ids = self
            .graph
            .edges_by_kind
            .values()
            .flatten()
            .filter(|edge| edge.from == node_id || edge.to == node_id)
            .map(|edge| edge.id)
            .collect::<Vec<_>>();
        for id in ids {
            let _ = self.delete_edge(id);
        }
    }
}

impl GraphReader for MemGraphStore {
    fn get_file(&self, id: FileNodeId) -> crate::Result<Option<super::FileNode>> {
        self.graph.get_file(id)
    }

    fn get_symbol(&self, id: SymbolNodeId) -> crate::Result<Option<super::SymbolNode>> {
        self.graph.get_symbol(id)
    }

    fn get_concept(&self, id: crate::ConceptNodeId) -> crate::Result<Option<super::ConceptNode>> {
        self.graph.get_concept(id)
    }

    fn file_by_path(&self, path: &str) -> crate::Result<Option<super::FileNode>> {
        self.graph.file_by_path(path)
    }

    fn outbound(&self, from: NodeId, kind: Option<EdgeKind>) -> crate::Result<Vec<Edge>> {
        self.graph.outbound(from, kind)
    }

    fn inbound(&self, to: NodeId, kind: Option<EdgeKind>) -> crate::Result<Vec<Edge>> {
        self.graph.inbound(to, kind)
    }

    fn all_file_paths(&self) -> crate::Result<Vec<(String, FileNodeId)>> {
        self.graph.all_file_paths()
    }

    fn all_concept_paths(&self) -> crate::Result<Vec<(String, crate::ConceptNodeId)>> {
        self.graph.all_concept_paths()
    }

    fn all_symbol_names(&self) -> crate::Result<Vec<(SymbolNodeId, FileNodeId, String)>> {
        self.graph.all_symbol_names()
    }

    fn all_symbols_for_resolution(
        &self,
    ) -> crate::Result<
        Vec<(
            SymbolNodeId,
            FileNodeId,
            String,
            super::SymbolKind,
            super::Visibility,
            String,
        )>,
    > {
        self.graph.all_symbols_for_resolution()
    }

    fn symbols_for_file(&self, file_id: FileNodeId) -> crate::Result<Vec<SymbolNode>> {
        self.graph.symbols_for_file(file_id)
    }

    fn edges_owned_by(&self, file_id: FileNodeId) -> crate::Result<Vec<Edge>> {
        self.graph.edges_owned_by(file_id)
    }

    fn all_edges(&self) -> crate::Result<Vec<Edge>> {
        self.graph.all_edges()
    }
}

impl GraphStore for MemGraphStore {
    fn upsert_file(&mut self, node: super::FileNode) -> crate::Result<()> {
        if let Some(old) = self.graph.files.get(&node.id) {
            self.graph.files_by_path.remove(&old.path);
        }
        self.graph.files_by_path.insert(node.path.clone(), node.id);
        self.graph.files.insert(node.id, node);
        Ok(())
    }

    fn upsert_symbol(&mut self, node: SymbolNode) -> crate::Result<()> {
        if let Some(old) = self.graph.symbols.get(&node.id).cloned() {
            self.remove_symbol_indexes(&old);
        }
        self.index_symbol(&node);
        self.graph.symbols.insert(node.id, node);
        Ok(())
    }

    fn upsert_concept(&mut self, node: super::ConceptNode) -> crate::Result<()> {
        if let Some(old) = self.graph.concepts.get(&node.id) {
            self.graph.concepts_by_path.remove(&old.path);
        }
        self.graph
            .concepts_by_path
            .insert(node.path.clone(), node.id);
        self.graph.concepts.insert(node.id, node);
        Ok(())
    }

    fn insert_edge(&mut self, edge: Edge) -> crate::Result<()> {
        if let Some(old) = self
            .graph
            .edges_by_kind
            .values()
            .flatten()
            .find(|candidate| candidate.id == edge.id)
            .cloned()
        {
            self.remove_edge_indexes(&old);
        }
        self.index_edge(&edge);
        Ok(())
    }

    fn delete_edge(&mut self, edge_id: EdgeId) -> crate::Result<()> {
        if let Some(edge) = self
            .graph
            .edges_by_kind
            .values()
            .flatten()
            .find(|candidate| candidate.id == edge_id)
            .cloned()
        {
            self.remove_edge_indexes(&edge);
        }
        Ok(())
    }

    fn delete_edges_by_kind(&mut self, kind: EdgeKind) -> crate::Result<usize> {
        let edges = self.graph.edges_by_kind.remove(&kind).unwrap_or_default();
        let count = edges.len();
        for edge in edges {
            remove_edge_from_bucket(self.graph.edges_by_from.get_mut(&edge.from), edge.id);
            remove_edge_from_bucket(self.graph.edges_by_to.get_mut(&edge.to), edge.id);
        }
        Ok(count)
    }

    fn delete_node(&mut self, id: NodeId) -> crate::Result<()> {
        self.remove_incident_edges(id);
        match id {
            NodeId::File(file_id) => {
                if let Some(file) = self.graph.files.remove(&file_id) {
                    self.graph.files_by_path.remove(&file.path);
                }
                for symbol in self.graph.symbols_for_file(file_id)? {
                    self.delete_node(NodeId::Symbol(symbol.id))?;
                }
            }
            NodeId::Symbol(symbol_id) => {
                if let Some(symbol) = self.graph.symbols.remove(&symbol_id) {
                    self.remove_symbol_indexes(&symbol);
                }
            }
            NodeId::Concept(concept_id) => {
                if let Some(concept) = self.graph.concepts.remove(&concept_id) {
                    self.graph.concepts_by_path.remove(&concept.path);
                }
            }
        }
        Ok(())
    }

    fn begin(&mut self) -> crate::Result<()> {
        self.transaction_backup = Some(self.graph.clone());
        Ok(())
    }

    fn commit(&mut self) -> crate::Result<()> {
        self.transaction_backup = None;
        self.graph.published_at = SystemTime::now();
        Ok(())
    }

    fn rollback(&mut self) -> crate::Result<()> {
        if let Some(graph) = self.transaction_backup.take() {
            self.graph = graph;
        }
        Ok(())
    }

    fn latest_drift_revision(&self) -> crate::Result<Option<String>> {
        Ok(self.drift_scores.keys().max().cloned())
    }

    fn write_drift_scores(
        &mut self,
        scores: &[(EdgeId, f32)],
        revision: &str,
    ) -> crate::Result<()> {
        self.drift_scores
            .insert(revision.to_string(), scores.to_vec());
        Ok(())
    }

    fn read_drift_scores(&self, revision: &str) -> crate::Result<Vec<(EdgeId, f32)>> {
        Ok(self.drift_scores.get(revision).cloned().unwrap_or_default())
    }

    fn truncate_drift_scores(&self, _older_than_revision: &str) -> crate::Result<usize> {
        Ok(0)
    }

    fn has_any_drift_scores(&self) -> crate::Result<bool> {
        Ok(!self.drift_scores.is_empty())
    }

    fn latest_fingerprint_revision(&self) -> crate::Result<Option<String>> {
        Ok(self.fingerprints.keys().max().cloned())
    }

    fn write_fingerprints(
        &mut self,
        fingerprints: &[(FileNodeId, StructuralFingerprint)],
        revision: &str,
    ) -> crate::Result<()> {
        self.fingerprints.insert(
            revision.to_string(),
            fingerprints.iter().cloned().collect::<HashMap<_, _>>(),
        );
        Ok(())
    }

    fn read_fingerprints(
        &self,
        revision: &str,
    ) -> crate::Result<HashMap<FileNodeId, StructuralFingerprint>> {
        Ok(self.fingerprints.get(revision).cloned().unwrap_or_default())
    }

    fn truncate_fingerprints(&self, _older_than_revision: &str) -> crate::Result<usize> {
        Ok(0)
    }

    fn next_compile_revision(&mut self) -> crate::Result<u64> {
        self.next_revision += 1;
        Ok(self.next_revision)
    }

    fn retire_symbol(&mut self, id: SymbolNodeId, revision: u64) -> crate::Result<()> {
        if let Some(mut symbol) = self.graph.symbols.get(&id).cloned() {
            self.remove_symbol_indexes(&symbol);
            symbol.retired_at_rev = Some(revision);
            self.graph.symbols.insert(id, symbol);
        }
        Ok(())
    }

    fn retire_edge(&mut self, id: EdgeId, revision: u64) -> crate::Result<()> {
        if let Some(mut edge) = self
            .graph
            .edges_by_kind
            .values()
            .flatten()
            .find(|candidate| candidate.id == id)
            .cloned()
        {
            self.remove_edge_indexes(&edge);
            edge.retired_at_rev = Some(revision);
        }
        Ok(())
    }

    fn unretire_symbol(&mut self, id: SymbolNodeId, revision: u64) -> crate::Result<()> {
        if let Some(mut symbol) = self.graph.symbols.get(&id).cloned() {
            symbol.retired_at_rev = None;
            symbol.last_observed_rev = Some(revision);
            self.index_symbol(&symbol);
            self.graph.symbols.insert(id, symbol);
        }
        Ok(())
    }
}

fn push_sorted_unique<T: Ord + Copy>(items: &mut Vec<T>, item: T) {
    if !items.contains(&item) {
        items.push(item);
        items.sort();
    }
}

fn push_edge_sorted(items: &mut Vec<Edge>, edge: &Edge) {
    if !items.iter().any(|candidate| candidate.id == edge.id) {
        items.push(edge.clone());
        items.sort_by_key(|candidate| candidate.id);
    }
}

fn remove_edge_from_bucket(bucket: Option<&mut Vec<Edge>>, edge_id: EdgeId) {
    if let Some(edges) = bucket {
        edges.retain(|edge| edge.id != edge_id);
    }
}
