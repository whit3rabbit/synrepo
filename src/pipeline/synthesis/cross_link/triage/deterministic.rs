//! Deterministic prefilter for cross-link generation.
//!
//! Filters by identifier name matching and graph distance to bound fan-out.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::core::ids::{FileNodeId, NodeId, SymbolNodeId};
use crate::overlay::OverlayEdgeKind;
#[allow(unused_imports)]
use crate::structure::graph::{
    Epistemic, GraphReader, GraphStore, SymbolKind, SymbolNode, Visibility,
};

use super::super::{CandidatePair, TriageSource};
use super::TriageScope;

/// Maximum graph hop count a surviving pair may have. Keeps the generation
/// pass O(concepts * matches-per-concept) rather than O(concepts * symbols).
pub const DEFAULT_DISTANCE_CUTOFF: u32 = 2;

/// Lower bound on identifier length considered for name-match. Prevents
/// pathological matches on short tokens like `id` or `fn`.
pub const MIN_IDENT_LEN: usize = 4;

/// Prefiltered candidate pairs ready for LLM verification.
///
/// Name-match: for each concept node, the function collects identifiers that
/// appear in its title, aliases, and decision body (when present) that also
/// appear as a symbol `qualified_name` tail component. Pairs with no
/// identifier overlap are discarded.
///
/// Graph distance: a BFS over the graph's observed edges (in either
/// direction) bounds the hop count. Pairs whose shortest path exceeds
/// `scope.distance_cutoff` are discarded.
pub fn candidate_pairs(
    graph: &dyn GraphStore,
    scope: &TriageScope,
) -> crate::Result<Vec<CandidatePair>> {
    if scope.concepts.is_empty() {
        return Ok(Vec::new());
    }

    let symbols = graph.all_symbol_names()?;
    let index = build_symbol_index(&symbols);

    let mut out = Vec::new();
    for concept_id in &scope.concepts {
        let NodeId::Concept(concept_node_id) = concept_id else {
            continue;
        };
        let Some(concept) = graph.get_concept(*concept_node_id)? else {
            continue;
        };

        let identifiers = collect_identifiers(&concept);
        if identifiers.is_empty() {
            continue;
        }

        let mut matched: HashSet<SymbolNodeId> = HashSet::new();
        for ident in &identifiers {
            if let Some(ids) = index.get(ident.as_str()) {
                for id in ids {
                    matched.insert(*id);
                }
            }
        }
        if matched.is_empty() {
            continue;
        }

        // Cap BFS work per concept to keep the prefilter bounded. The cutoff
        // is already small but the concept could still reach many symbols
        // through dense imports.
        let distances = bfs_distances(graph, *concept_id, &matched, scope.distance_cutoff)?;
        for sym_id in matched {
            let Some(dist) = distances.get(&sym_id).copied() else {
                continue;
            };
            out.push(CandidatePair {
                from: *concept_id,
                to: NodeId::Symbol(sym_id),
                kind: OverlayEdgeKind::References,
                graph_distance: dist,
                source: TriageSource::Deterministic,
            });
        }
    }
    Ok(out)
}

fn collect_identifiers(concept: &crate::structure::graph::ConceptNode) -> HashSet<String> {
    let mut out = HashSet::new();
    for word in tokenize(&concept.title) {
        out.insert(word);
    }
    for alias in &concept.aliases {
        for word in tokenize(alias) {
            out.insert(word);
        }
    }
    if let Some(body) = &concept.decision_body {
        for word in tokenize(body) {
            out.insert(word);
        }
    }
    if let Some(summary) = &concept.summary {
        for word in tokenize(summary) {
            out.insert(word);
        }
    }
    out.retain(|w| w.len() >= MIN_IDENT_LEN);
    out
}

/// Split a prose string into candidate identifier tokens.
fn tokenize(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for c in text.chars() {
        if c.is_alphanumeric() || c == '_' {
            cur.push(c);
        } else if !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Build an index from trailing-path-component identifiers to symbol IDs.
/// Matches against the last `::` or `.`-separated segment of the qualified
/// name so `auth::authenticate` and `module.Class.authenticate` both
/// surface on the identifier `authenticate`.
fn build_symbol_index(
    symbols: &[(SymbolNodeId, FileNodeId, String)],
) -> HashMap<String, Vec<SymbolNodeId>> {
    let mut out: HashMap<String, Vec<SymbolNodeId>> = HashMap::new();
    for (id, _, qualified_name) in symbols {
        let tail = qualified_name
            .rsplit([':', '.'])
            .next()
            .unwrap_or(qualified_name);
        if tail.len() < MIN_IDENT_LEN {
            continue;
        }
        out.entry(tail.to_string()).or_default().push(*id);
    }
    out
}

/// BFS the undirected view of the graph from `start` up to `max_distance`,
/// recording distances only for nodes in `targets`. Early-out once every
/// target is found.
pub(super) fn bfs_distances(
    graph: &dyn GraphStore,
    start: NodeId,
    targets: &HashSet<SymbolNodeId>,
    max_distance: u32,
) -> crate::Result<HashMap<SymbolNodeId, u32>> {
    let mut found: HashMap<SymbolNodeId, u32> = HashMap::new();
    let mut seen: HashSet<NodeId> = HashSet::new();
    seen.insert(start);
    let mut queue: VecDeque<(NodeId, u32)> = VecDeque::new();
    queue.push_back((start, 0));

    while let Some((node, dist)) = queue.pop_front() {
        if let NodeId::Symbol(sym_id) = node {
            if targets.contains(&sym_id) && !found.contains_key(&sym_id) {
                found.insert(sym_id, dist);
                if found.len() == targets.len() {
                    return Ok(found);
                }
            }
        }
        if dist >= max_distance {
            continue;
        }

        let outs = graph.outbound(node, None)?;
        let ins = graph.inbound(node, None)?;
        for edge in outs.into_iter().chain(ins) {
            let next = if edge.from == node {
                edge.to
            } else {
                edge.from
            };
            if seen.insert(next) {
                queue.push_back((next, dist + 1));
            }
        }
    }

    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ids::{ConceptNodeId, EdgeId, FileNodeId, SymbolNodeId};
    use crate::core::provenance::Provenance;

    fn test_provenance() -> Provenance {
        Provenance::structural("triage-test", "rev", Vec::new())
    }
    use crate::structure::graph::{
        ConceptNode, Edge, EdgeKind, Epistemic, FileNode, SymbolKind, SymbolNode,
    };

    /// Tiny in-memory graph for prefilter tests. Only implements the methods
    /// triage uses.
    struct MemGraph {
        files: HashMap<FileNodeId, FileNode>,
        symbols: HashMap<SymbolNodeId, SymbolNode>,
        concepts: HashMap<ConceptNodeId, ConceptNode>,
        edges: Vec<Edge>,
    }

    impl MemGraph {
        fn new() -> Self {
            Self {
                files: HashMap::new(),
                symbols: HashMap::new(),
                concepts: HashMap::new(),
                edges: Vec::new(),
            }
        }
    }

    impl GraphReader for MemGraph {
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
            Ok(self.files.values().find(|f| f.path == path).cloned())
        }
        fn outbound(&self, from: NodeId, kind: Option<EdgeKind>) -> crate::Result<Vec<Edge>> {
            Ok(self
                .edges
                .iter()
                .filter(|e| e.from == from && kind.is_none_or(|k| e.kind == k))
                .cloned()
                .collect())
        }
        fn inbound(&self, to: NodeId, kind: Option<EdgeKind>) -> crate::Result<Vec<Edge>> {
            Ok(self
                .edges
                .iter()
                .filter(|e| e.to == to && kind.is_none_or(|k| e.kind == k))
                .cloned()
                .collect())
        }
        fn all_file_paths(&self) -> crate::Result<Vec<(String, FileNodeId)>> {
            Ok(self
                .files
                .values()
                .map(|f| (f.path.clone(), f.id))
                .collect())
        }
        fn all_concept_paths(&self) -> crate::Result<Vec<(String, ConceptNodeId)>> {
            Ok(self
                .concepts
                .values()
                .map(|c| (c.path.clone(), c.id))
                .collect())
        }
        fn all_symbol_names(&self) -> crate::Result<Vec<(SymbolNodeId, FileNodeId, String)>> {
            Ok(self
                .symbols
                .values()
                .map(|s| (s.id, s.file_id, s.qualified_name.clone()))
                .collect())
        }
    }

    impl GraphStore for MemGraph {
        fn upsert_file(&mut self, node: FileNode) -> crate::Result<()> {
            self.files.insert(node.id, node);
            Ok(())
        }
        fn upsert_symbol(&mut self, node: SymbolNode) -> crate::Result<()> {
            self.symbols.insert(node.id, node);
            Ok(())
        }
        fn upsert_concept(&mut self, node: ConceptNode) -> crate::Result<()> {
            self.concepts.insert(node.id, node);
            Ok(())
        }
        fn insert_edge(&mut self, edge: Edge) -> crate::Result<()> {
            self.edges.push(edge);
            Ok(())
        }
        fn delete_edge(&mut self, id: EdgeId) -> crate::Result<()> {
            self.edges.retain(|e| e.id != id);
            Ok(())
        }
        fn delete_edges_by_kind(&mut self, kind: EdgeKind) -> crate::Result<usize> {
            let before = self.edges.len();
            self.edges.retain(|e| e.kind != kind);
            Ok(before - self.edges.len())
        }
        fn delete_node(&mut self, _id: NodeId) -> crate::Result<()> {
            Ok(())
        }
        fn commit(&mut self) -> crate::Result<()> {
            Ok(())
        }
    }

    fn file_node(id: u64, path: &str) -> FileNode {
        FileNode {
            id: FileNodeId(id as u128),
            path: path.into(),
            path_history: Vec::new(),
            content_hash: format!("hash-{id}"),
            size_bytes: 0,
            language: Some("rust".into()),
            inline_decisions: Vec::new(),
            last_observed_rev: None,
            epistemic: Epistemic::ParserObserved,
            provenance: test_provenance(),
        }
    }

    fn sym_node(id: u64, file_id: FileNodeId, qname: &str) -> SymbolNode {
        SymbolNode {
            id: SymbolNodeId(id as u128),
            file_id,
            qualified_name: qname.into(),
            display_name: qname.rsplit("::").next().unwrap_or(qname).into(),
            kind: SymbolKind::Function,
            visibility: Visibility::Public,
            body_byte_range: (0, 1),
            body_hash: "b".into(),
            signature: None,
            doc_comment: None,
            first_seen_rev: None,
            last_modified_rev: None,
            last_observed_rev: None,
            retired_at_rev: None,
            epistemic: Epistemic::ParserObserved,
            provenance: test_provenance(),
        }
    }

    fn concept(id: u64, path: &str, title: &str, body: Option<&str>) -> ConceptNode {
        ConceptNode {
            id: ConceptNodeId(id as u128),
            path: path.into(),
            title: title.into(),
            aliases: Vec::new(),
            summary: None,
            status: None,
            decision_body: body.map(|s| s.into()),
            last_observed_rev: None,
            epistemic: Epistemic::HumanDeclared,
            provenance: test_provenance(),
        }
    }

    fn edge_between(id: u64, from: NodeId, to: NodeId, kind: EdgeKind) -> Edge {
        Edge {
            id: crate::core::ids::EdgeId(id as u128),
            from,
            to,
            kind,
            owner_file_id: None,
            last_observed_rev: None,
            retired_at_rev: None,
            epistemic: Epistemic::ParserObserved,
            drift_score: 0.0,
            provenance: test_provenance(),
        }
    }

    #[test]
    fn returns_empty_when_name_match_fails() {
        let mut g = MemGraph::new();
        let file = file_node(1, "src/auth.rs");
        g.upsert_file(file.clone()).unwrap();
        let sym = sym_node(10, file.id, "auth::handshake");
        g.upsert_symbol(sym.clone()).unwrap();
        let con = concept(100, "docs/concepts/unrelated.md", "Logging pipeline", None);
        g.upsert_concept(con.clone()).unwrap();

        let scope = TriageScope {
            concepts: vec![NodeId::Concept(con.id)],
            distance_cutoff: DEFAULT_DISTANCE_CUTOFF,
        };
        let pairs = candidate_pairs(&g, &scope).unwrap();
        assert!(pairs.is_empty());
    }

    #[test]
    fn discards_pairs_beyond_distance_cutoff() {
        let mut g = MemGraph::new();
        let file = file_node(1, "src/auth.rs");
        g.upsert_file(file.clone()).unwrap();
        let sym = sym_node(10, file.id, "auth::authenticate");
        g.upsert_symbol(sym.clone()).unwrap();
        // Concept mentions the identifier but is not graph-linked to the
        // symbol; BFS finds nothing within the cutoff.
        let con = concept(
            100,
            "docs/concepts/auth.md",
            "Authenticate flow",
            Some("the authenticate function"),
        );
        g.upsert_concept(con.clone()).unwrap();

        let scope = TriageScope {
            concepts: vec![NodeId::Concept(con.id)],
            distance_cutoff: 1,
        };
        let pairs = candidate_pairs(&g, &scope).unwrap();
        assert!(
            pairs.is_empty(),
            "pair should be dropped when unreachable within the distance cutoff"
        );
    }

    #[test]
    fn keeps_pair_when_both_filters_pass() {
        let mut g = MemGraph::new();
        let file = file_node(1, "src/auth.rs");
        g.upsert_file(file.clone()).unwrap();
        let sym = sym_node(10, file.id, "auth::authenticate");
        g.upsert_symbol(sym.clone()).unwrap();
        let con = concept(
            100,
            "docs/concepts/auth.md",
            "Authenticate flow",
            Some("the authenticate function"),
        );
        g.upsert_concept(con.clone()).unwrap();
        // Link concept -> file -> symbol via Mentions + Defines (distance 2).
        g.insert_edge(edge_between(
            1,
            NodeId::Concept(con.id),
            NodeId::File(file.id),
            EdgeKind::Mentions,
        ))
        .unwrap();
        g.insert_edge(edge_between(
            2,
            NodeId::File(file.id),
            NodeId::Symbol(sym.id),
            EdgeKind::Defines,
        ))
        .unwrap();

        let scope = TriageScope {
            concepts: vec![NodeId::Concept(con.id)],
            distance_cutoff: 2,
        };
        let pairs = candidate_pairs(&g, &scope).unwrap();
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].from, NodeId::Concept(con.id));
        assert_eq!(pairs[0].to, NodeId::Symbol(sym.id));
        assert!(pairs[0].graph_distance <= 2);
    }
}
