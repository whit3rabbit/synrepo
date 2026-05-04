//! Semantic prefilter for cross-link candidate generation.
//!
//! Uses embedding similarity to match concepts to symbols, then applies
//! graph distance filtering.

#![cfg(feature = "semantic-triage")]

use std::collections::{HashMap, HashSet};

use crate::config::Config;
use crate::core::ids::{ConceptNodeId, NodeId, SymbolNodeId};
use crate::overlay::OverlayEdgeKind;
use crate::structure::graph::GraphStore;
use crate::substrate::embedding::FlatVecIndex;

use super::super::{CandidatePair, TriageSource};
use super::deterministic::bfs_distances;
use super::TriageScope;

/// Semantic prefilter for cross-link candidate generation.
///
/// Runs after the deterministic prefilter to catch pairs that the deterministic
/// pass missed due to lack of lexical overlap. Uses embedding similarity to
/// match concepts to symbols, then applies graph distance filtering.
///
/// Returns pairs that exceed the similarity threshold and have acceptable graph distance.
pub fn semantic_candidates(
    graph: &dyn GraphStore,
    index: &FlatVecIndex,
    config: &Config,
    scope: &TriageScope,
) -> crate::Result<Vec<CandidatePair>> {
    if scope.concepts.is_empty() || index.is_empty() {
        return Ok(Vec::new());
    }

    // embed_text() will now return a hard error if the session is missing or fails,
    // which propagates up. This ensures that enabled semantic triage is strict.

    let threshold = config.semantic_similarity_threshold as f32;

    // Get concept texts for embedding
    let mut concept_texts: HashMap<ConceptNodeId, String> = HashMap::new();
    for concept_id in &scope.concepts {
        let NodeId::Concept(cid) = concept_id else {
            continue;
        };
        if let Some(concept) = graph.get_concept(*cid)? {
            let mut text_parts = vec![concept.title.clone()];
            text_parts.extend(concept.aliases.clone());
            if let Some(summary) = &concept.summary {
                text_parts.push(summary.clone());
            }
            // Also include decision body if present
            if let Some(body) = &concept.decision_body {
                text_parts.push(body.clone());
            }
            concept_texts.insert(*cid, text_parts.join(" "));
        }
    }

    if concept_texts.is_empty() {
        return Ok(Vec::new());
    }

    // For each concept, embed it and query the index for similar symbols
    let mut out = Vec::new();

    for (concept_id, concept_text) in &concept_texts {
        // Embed the concept text (propagates errors in strict mode)
        let query_vector = index.embed_text(concept_text)?;

        // Query for top matches (more than needed for distance filtering)
        let top_matches = index.query(&query_vector, 20);

        // Filter to symbol chunks only and above threshold.
        let mut matched = HashSet::new();
        for (chunk_id, similarity) in top_matches {
            if similarity < threshold {
                continue;
            }

            let Some(symbol_id) = index.chunk_to_symbol_id(&chunk_id) else {
                continue;
            };
            matched.insert(symbol_id);
        }

        push_reachable_semantic_pairs(
            graph,
            *concept_id,
            &matched,
            scope.distance_cutoff,
            &mut out,
        )?;
    }

    Ok(out)
}

fn push_reachable_semantic_pairs(
    graph: &dyn GraphStore,
    concept_id: ConceptNodeId,
    matched: &HashSet<SymbolNodeId>,
    distance_cutoff: u32,
    out: &mut Vec<CandidatePair>,
) -> crate::Result<()> {
    if matched.is_empty() {
        return Ok(());
    }

    let distances = bfs_distances(graph, NodeId::Concept(concept_id), matched, distance_cutoff)?;

    let mut symbols = matched.iter().copied().collect::<Vec<_>>();
    symbols.sort_by_key(|id| id.0);
    for sym_id in symbols {
        let Some(dist) = distances.get(&sym_id).copied() else {
            continue;
        };
        out.push(CandidatePair {
            from: NodeId::Concept(concept_id),
            to: NodeId::Symbol(sym_id),
            kind: OverlayEdgeKind::References,
            graph_distance: dist,
            source: TriageSource::Semantic,
        });
    }
    Ok(())
}

/// Wrapper that runs deterministic prefilter, then semantic prefilter on unmatched concepts.
/// Returns all candidates (deterministic + semantic).
pub fn all_candidates(
    graph: &dyn GraphStore,
    index: Option<&FlatVecIndex>,
    config: &Config,
    scope: &TriageScope,
) -> crate::Result<Vec<CandidatePair>> {
    // First run deterministic
    let deterministic = super::deterministic::candidate_pairs(graph, scope)?;

    // If no semantic index, just return deterministic
    let Some(index) = index else {
        return Ok(deterministic);
    };

    // Get the concepts that were matched by deterministic
    let matched_concepts: HashSet<ConceptNodeId> = deterministic
        .iter()
        .filter_map(|p| {
            if let NodeId::Concept(cid) = p.from {
                Some(cid)
            } else {
                None
            }
        })
        .collect();

    // Create a scope for unmatched concepts
    let unmatched_concepts: Vec<NodeId> = scope
        .concepts
        .iter()
        .filter(|c| {
            if let NodeId::Concept(cid) = c {
                !matched_concepts.contains(cid)
            } else {
                true
            }
        })
        .cloned()
        .collect();

    if unmatched_concepts.is_empty() {
        return Ok(deterministic);
    }

    let semantic_scope = TriageScope {
        concepts: unmatched_concepts,
        distance_cutoff: scope.distance_cutoff,
    };

    // Run semantic prefilter on unmatched concepts
    let semantic = semantic_candidates(graph, index, config, &semantic_scope)?;

    // Combine results
    let mut all = deterministic;
    all.extend(semantic);
    Ok(all)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ids::{EdgeId, FileNodeId};
    use crate::core::provenance::Provenance;
    use crate::structure::graph::{
        ConceptNode, Edge, EdgeKind, Epistemic, FileNode, GraphReader, SymbolNode,
    };
    use std::collections::HashSet;

    struct MemGraph {
        edges: Vec<Edge>,
    }

    impl GraphReader for MemGraph {
        fn get_file(&self, _id: FileNodeId) -> crate::Result<Option<FileNode>> {
            Ok(None)
        }

        fn get_symbol(&self, _id: SymbolNodeId) -> crate::Result<Option<SymbolNode>> {
            Ok(None)
        }

        fn get_concept(&self, _id: ConceptNodeId) -> crate::Result<Option<ConceptNode>> {
            Ok(None)
        }

        fn file_by_path(&self, _path: &str) -> crate::Result<Option<FileNode>> {
            Ok(None)
        }

        fn outbound(&self, from: NodeId, kind: Option<EdgeKind>) -> crate::Result<Vec<Edge>> {
            Ok(self
                .edges
                .iter()
                .filter(|edge| edge.from == from && kind.is_none_or(|kind| edge.kind == kind))
                .cloned()
                .collect())
        }

        fn inbound(&self, to: NodeId, kind: Option<EdgeKind>) -> crate::Result<Vec<Edge>> {
            Ok(self
                .edges
                .iter()
                .filter(|edge| edge.to == to && kind.is_none_or(|kind| edge.kind == kind))
                .cloned()
                .collect())
        }

        fn all_file_paths(&self) -> crate::Result<Vec<(String, FileNodeId)>> {
            Ok(Vec::new())
        }

        fn all_concept_paths(&self) -> crate::Result<Vec<(String, ConceptNodeId)>> {
            Ok(Vec::new())
        }

        fn all_symbol_names(&self) -> crate::Result<Vec<(SymbolNodeId, FileNodeId, String)>> {
            Ok(Vec::new())
        }
    }

    impl GraphStore for MemGraph {
        fn upsert_file(&mut self, _node: FileNode) -> crate::Result<()> {
            Ok(())
        }

        fn upsert_symbol(&mut self, _node: SymbolNode) -> crate::Result<()> {
            Ok(())
        }

        fn upsert_concept(&mut self, _node: ConceptNode) -> crate::Result<()> {
            Ok(())
        }

        fn insert_edge(&mut self, edge: Edge) -> crate::Result<()> {
            self.edges.push(edge);
            Ok(())
        }

        fn delete_node(&mut self, _id: NodeId) -> crate::Result<()> {
            Ok(())
        }

        fn commit(&mut self) -> crate::Result<()> {
            Ok(())
        }
    }

    fn edge(id: u128, from: NodeId, to: NodeId) -> Edge {
        Edge {
            id: EdgeId(id),
            from,
            to,
            kind: EdgeKind::References,
            owner_file_id: None,
            last_observed_rev: None,
            retired_at_rev: None,
            epistemic: Epistemic::ParserObserved,
            provenance: Provenance::structural("test", "rev001", Vec::new()),
        }
    }

    #[test]
    fn semantic_pairs_batch_multiple_symbols_for_one_concept() {
        let concept_id = ConceptNodeId(1);
        let sym_a = SymbolNodeId(2);
        let sym_b = SymbolNodeId(3);
        let graph = MemGraph {
            edges: vec![
                edge(10, NodeId::Concept(concept_id), NodeId::Symbol(sym_a)),
                edge(11, NodeId::Concept(concept_id), NodeId::Symbol(sym_b)),
            ],
        };
        let matched = HashSet::from([sym_a, sym_b]);
        let mut out = Vec::new();

        push_reachable_semantic_pairs(&graph, concept_id, &matched, 1, &mut out).unwrap();

        let targets = out.iter().map(|pair| pair.to).collect::<Vec<_>>();
        assert_eq!(
            targets,
            vec![NodeId::Symbol(sym_a), NodeId::Symbol(sym_b)],
            "semantic batch should emit both reachable matches"
        );
        assert!(out.iter().all(|pair| pair.source == TriageSource::Semantic));
    }
}
