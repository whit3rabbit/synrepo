//! Internal helpers: tokenization, symbol indexing, BFS distance.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::core::ids::{FileNodeId, NodeId, SymbolNodeId};
#[allow(unused_imports)]
use crate::structure::graph::{
    Epistemic, GraphReader, GraphStore, SymbolKind, SymbolNode, Visibility,
};

use super::MIN_IDENT_LEN;

pub(super) fn collect_identifiers(
    concept: &crate::structure::graph::ConceptNode,
) -> HashSet<String> {
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
pub(super) fn tokenize(text: &str) -> Vec<String> {
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
pub(super) fn build_symbol_index(
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
pub(in crate::pipeline::explain::cross_link::triage) fn bfs_distances(
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
