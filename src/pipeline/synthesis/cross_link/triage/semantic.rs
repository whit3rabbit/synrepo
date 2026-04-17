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

        // Filter to symbol chunks only and above threshold
        for (chunk_id, similarity) in top_matches {
            if similarity < threshold {
                continue;
            }

            // Skip if not a symbol chunk
            let Some(symbol_id) = index.chunk_to_symbol_id(&chunk_id) else {
                continue;
            };

            // Check graph distance
            let distances = bfs_distances(
                graph,
                NodeId::Concept(*concept_id),
                &HashSet::from([symbol_id]),
                scope.distance_cutoff,
            )?;

            if let Some(dist) = distances.get(&symbol_id) {
                out.push(CandidatePair {
                    from: NodeId::Concept(*concept_id),
                    to: NodeId::Symbol(symbol_id),
                    kind: OverlayEdgeKind::References,
                    graph_distance: *dist,
                    source: TriageSource::Semantic,
                });
            }
        }
    }

    Ok(out)
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
