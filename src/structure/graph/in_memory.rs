use std::collections::HashMap;
use std::mem::size_of;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::core::ids::{ConceptNodeId, FileNodeId, NodeId, SymbolNodeId};
use crate::core::provenance::{Provenance, SourceRef};

use super::{ConceptNode, Edge, EdgeKind, FileNode, GraphReader, SymbolNode};

/// Immutable in-memory snapshot of the active graph.
#[derive(Clone, Debug)]
pub struct Graph {
    /// Monotonic epoch assigned when the snapshot is published.
    pub snapshot_epoch: u64,
    /// Time when this snapshot was most recently published.
    pub published_at: SystemTime,
    /// Active file nodes keyed by stable ID.
    pub files: HashMap<FileNodeId, FileNode>,
    /// Reverse lookup from repo-relative file path to file node ID.
    pub files_by_path: HashMap<String, FileNodeId>,
    /// Active symbol nodes keyed by stable ID.
    pub symbols: HashMap<SymbolNodeId, SymbolNode>,
    /// File-owned symbol membership index.
    pub symbols_by_file: HashMap<FileNodeId, Vec<SymbolNodeId>>,
    /// Short-name lookup index for symbol resolution.
    pub symbols_by_short_name: HashMap<String, Vec<SymbolNodeId>>,
    /// Active concept nodes keyed by stable ID.
    pub concepts: HashMap<ConceptNodeId, ConceptNode>,
    /// Reverse lookup from concept path to concept node ID.
    pub concepts_by_path: HashMap<String, ConceptNodeId>,
    /// Outbound edge adjacency by source node.
    pub edges_by_from: HashMap<NodeId, Vec<Edge>>,
    /// Inbound edge adjacency by target node.
    pub edges_by_to: HashMap<NodeId, Vec<Edge>>,
    /// Edge buckets by kind for aggregate scans.
    pub edges_by_kind: HashMap<EdgeKind, Vec<Edge>>,
}

impl Default for Graph {
    fn default() -> Self {
        Self::empty()
    }
}

impl Graph {
    /// Construct an empty unpublished snapshot.
    pub fn empty() -> Self {
        Self {
            snapshot_epoch: 0,
            published_at: UNIX_EPOCH,
            files: HashMap::new(),
            files_by_path: HashMap::new(),
            symbols: HashMap::new(),
            symbols_by_file: HashMap::new(),
            symbols_by_short_name: HashMap::new(),
            concepts: HashMap::new(),
            concepts_by_path: HashMap::new(),
            edges_by_from: HashMap::new(),
            edges_by_to: HashMap::new(),
            edges_by_kind: HashMap::new(),
        }
    }

    /// Materialize a full in-memory snapshot from a read-only graph source.
    pub fn from_store(reader: &dyn GraphReader) -> crate::Result<Graph> {
        let mut graph = Graph::empty();

        for (path, file_id) in reader.all_file_paths()? {
            if let Some(file) = reader.get_file(file_id)? {
                graph.files_by_path.insert(path, file_id);
                graph.files.insert(file_id, file);
            }
        }

        for (path, concept_id) in reader.all_concept_paths()? {
            if let Some(concept) = reader.get_concept(concept_id)? {
                graph.concepts_by_path.insert(path, concept_id);
                graph.concepts.insert(concept_id, concept);
            }
        }

        for (symbol_id, _file_id, _qname) in reader.all_symbol_names()? {
            if let Some(symbol) = reader.get_symbol(symbol_id)? {
                graph
                    .symbols_by_file
                    .entry(symbol.file_id)
                    .or_default()
                    .push(symbol_id);
                graph
                    .symbols_by_short_name
                    .entry(symbol.display_name.clone())
                    .or_default()
                    .push(symbol_id);
                graph.symbols.insert(symbol_id, symbol);
            }
        }

        for edge in reader.active_edges()? {
            graph
                .edges_by_from
                .entry(edge.from)
                .or_default()
                .push(edge.clone());
            graph
                .edges_by_to
                .entry(edge.to)
                .or_default()
                .push(edge.clone());
            graph.edges_by_kind.entry(edge.kind).or_default().push(edge);
        }

        for ids in graph.symbols_by_file.values_mut() {
            ids.sort();
        }
        for ids in graph.symbols_by_short_name.values_mut() {
            ids.sort();
        }
        for edges in graph.edges_by_from.values_mut() {
            edges.sort_by_key(|edge| edge.id);
        }
        for edges in graph.edges_by_to.values_mut() {
            edges.sort_by_key(|edge| edge.id);
        }
        for edges in graph.edges_by_kind.values_mut() {
            edges.sort_by_key(|edge| edge.id);
        }

        Ok(graph)
    }

    /// Approximate heap usage of the snapshot and its indexes.
    pub fn approx_bytes(&self) -> usize {
        let mut total = 0usize;

        total += self
            .files
            .values()
            .map(approx_file_node_bytes)
            .sum::<usize>();
        total += self
            .files_by_path
            .keys()
            .map(|path| path.capacity() + size_of::<FileNodeId>() + 24)
            .sum::<usize>();

        total += self
            .symbols
            .values()
            .map(approx_symbol_node_bytes)
            .sum::<usize>();
        total += self
            .symbols_by_file
            .values()
            .map(|ids| ids.capacity() * size_of::<SymbolNodeId>() + 24)
            .sum::<usize>();
        total += self
            .symbols_by_short_name
            .iter()
            .map(|(name, ids)| name.capacity() + ids.capacity() * size_of::<SymbolNodeId>() + 24)
            .sum::<usize>();

        total += self
            .concepts
            .values()
            .map(approx_concept_node_bytes)
            .sum::<usize>();
        total += self
            .concepts_by_path
            .keys()
            .map(|path| path.capacity() + size_of::<ConceptNodeId>() + 24)
            .sum::<usize>();

        total += self
            .edges_by_from
            .values()
            .map(|edges| edges.iter().map(approx_edge_bytes).sum::<usize>() + 24)
            .sum::<usize>();
        total += self
            .edges_by_to
            .values()
            .map(|edges| edges.iter().map(approx_edge_bytes).sum::<usize>() + 24)
            .sum::<usize>();
        total += self
            .edges_by_kind
            .values()
            .map(|edges| edges.iter().map(approx_edge_bytes).sum::<usize>() + 24)
            .sum::<usize>();

        total
    }
}

impl GraphReader for Graph {
    fn get_file(&self, id: FileNodeId) -> crate::Result<Option<FileNode>> {
        Ok(self.files.get(&id).cloned())
    }

    fn get_symbol(&self, id: SymbolNodeId) -> crate::Result<Option<SymbolNode>> {
        Ok(self.symbols.get(&id).cloned())
    }

    fn get_concept(&self, id: ConceptNodeId) -> crate::Result<Option<ConceptNode>> {
        Ok(self.concepts.get(&id).cloned())
    }

    fn file_by_path(&self, path: &str) -> crate::Result<Option<FileNode>> {
        Ok(self
            .files_by_path
            .get(path)
            .and_then(|id| self.files.get(id))
            .cloned())
    }

    fn outbound(&self, from: NodeId, kind: Option<EdgeKind>) -> crate::Result<Vec<Edge>> {
        let edges = self.edges_by_from.get(&from).cloned().unwrap_or_default();
        Ok(filter_edges_by_kind(edges, kind))
    }

    fn inbound(&self, to: NodeId, kind: Option<EdgeKind>) -> crate::Result<Vec<Edge>> {
        let edges = self.edges_by_to.get(&to).cloned().unwrap_or_default();
        Ok(filter_edges_by_kind(edges, kind))
    }

    fn all_file_paths(&self) -> crate::Result<Vec<(String, FileNodeId)>> {
        let mut out: Vec<_> = self
            .files_by_path
            .iter()
            .map(|(path, id)| (path.clone(), *id))
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(out)
    }

    fn all_concept_paths(&self) -> crate::Result<Vec<(String, ConceptNodeId)>> {
        let mut out: Vec<_> = self
            .concepts_by_path
            .iter()
            .map(|(path, id)| (path.clone(), *id))
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(out)
    }

    fn all_symbol_names(&self) -> crate::Result<Vec<(SymbolNodeId, FileNodeId, String)>> {
        let mut out: Vec<_> = self
            .symbols
            .values()
            .filter(|symbol| symbol.retired_at_rev.is_none())
            .map(|symbol| (symbol.id, symbol.file_id, symbol.qualified_name.clone()))
            .collect();
        out.sort_by_key(|(id, _, _)| *id);
        Ok(out)
    }

    fn symbols_for_file(&self, file_id: FileNodeId) -> crate::Result<Vec<SymbolNode>> {
        let mut out = self
            .symbols_by_file
            .get(&file_id)
            .into_iter()
            .flatten()
            .filter_map(|id| self.symbols.get(id).cloned())
            .collect::<Vec<_>>();
        out.sort_by_key(|symbol| symbol.id);
        Ok(out)
    }

    fn edges_owned_by(&self, file_id: FileNodeId) -> crate::Result<Vec<Edge>> {
        let mut out = self
            .edges_by_from
            .values()
            .flatten()
            .filter(|edge| edge.owner_file_id == Some(file_id))
            .cloned()
            .collect::<Vec<_>>();
        out.sort_by_key(|edge| edge.id);
        Ok(out)
    }

    fn all_edges(&self) -> crate::Result<Vec<Edge>> {
        let mut out = self
            .edges_by_kind
            .values()
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        out.sort_by_key(|edge| edge.id);
        Ok(out)
    }

    fn count_edges_by_kind(&self, kind: EdgeKind) -> crate::Result<usize> {
        Ok(self.edges_by_kind.get(&kind).map_or(0, Vec::len))
    }
}

fn filter_edges_by_kind(edges: Vec<Edge>, kind: Option<EdgeKind>) -> Vec<Edge> {
    match kind {
        Some(kind) => edges.into_iter().filter(|edge| edge.kind == kind).collect(),
        None => edges,
    }
}

fn approx_file_node_bytes(file: &FileNode) -> usize {
    size_of::<FileNode>()
        + file.path.capacity()
        + file
            .path_history
            .iter()
            .map(|path| path.capacity())
            .sum::<usize>()
        + file.content_hash.capacity()
        + file.language.as_ref().map_or(0, String::capacity)
        + file
            .inline_decisions
            .iter()
            .map(|decision| decision.capacity())
            .sum::<usize>()
        + approx_provenance_bytes(&file.provenance)
}

fn approx_symbol_node_bytes(symbol: &SymbolNode) -> usize {
    size_of::<SymbolNode>()
        + symbol.qualified_name.capacity()
        + symbol.display_name.capacity()
        + symbol.body_hash.capacity()
        + symbol.signature.as_ref().map_or(0, String::capacity)
        + symbol.doc_comment.as_ref().map_or(0, String::capacity)
        + symbol.first_seen_rev.as_ref().map_or(0, String::capacity)
        + symbol
            .last_modified_rev
            .as_ref()
            .map_or(0, String::capacity)
        + approx_provenance_bytes(&symbol.provenance)
}

fn approx_concept_node_bytes(concept: &ConceptNode) -> usize {
    size_of::<ConceptNode>()
        + concept.path.capacity()
        + concept.title.capacity()
        + concept
            .aliases
            .iter()
            .map(|alias| alias.capacity())
            .sum::<usize>()
        + concept.summary.as_ref().map_or(0, String::capacity)
        + concept.status.as_ref().map_or(0, String::capacity)
        + concept.decision_body.as_ref().map_or(0, String::capacity)
        + approx_provenance_bytes(&concept.provenance)
}

fn approx_edge_bytes(edge: &Edge) -> usize {
    size_of::<Edge>() + approx_provenance_bytes(&edge.provenance)
}

fn approx_provenance_bytes(provenance: &Provenance) -> usize {
    size_of::<Provenance>()
        + provenance.source_revision.capacity()
        + provenance.pass.capacity()
        + provenance
            .source_artifacts
            .iter()
            .map(approx_source_ref_bytes)
            .sum::<usize>()
}

fn approx_source_ref_bytes(source: &SourceRef) -> usize {
    size_of::<SourceRef>() + source.path.capacity() + source.content_hash.capacity()
}
