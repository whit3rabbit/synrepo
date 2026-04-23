use std::{collections::HashMap, path::Path};

use crate::{
    config::Config,
    core::ids::NodeId,
    overlay::{
        derive_link_freshness, ConfidenceTier, CrossLinkFreshness, OverlayEpistemic, OverlayLink,
        OverlayStore,
    },
    pipeline::explain::{
        build_cross_link_generator, score, CandidatePair, CandidateScope, CrossLinkGenerator,
    },
    store::{overlay::SqliteOverlayStore, sqlite::SqliteGraphStore},
};

use super::cross_link_verify::{current_endpoint_hash, verify_candidate_payload};

/// Summary of one cross-link generation pass.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct GenerationOutcome {
    /// Candidates persisted into the overlay store.
    pub inserted: usize,
    /// Candidate pairs skipped because the per-run cost limit was hit.
    pub blocked_pairs: usize,
}

/// Run the configured cross-link generation path using the real generator.
pub(super) fn run_cross_link_generation(
    repo_root: &Path,
    synrepo_dir: &Path,
    config: &Config,
    generate_new: bool,
    regenerate_stale: bool,
) -> crate::Result<GenerationOutcome> {
    let generator = build_cross_link_generator(
        config,
        config.commentary_cost_limit,
        config.cross_link_confidence_thresholds.into(),
    );
    run_cross_link_generation_with_generator(
        repo_root,
        synrepo_dir,
        config,
        generate_new,
        regenerate_stale,
        generator.as_ref(),
    )
}

/// Run the full cross-link generation pass using an injected generator.
pub(super) fn run_cross_link_generation_with_generator(
    repo_root: &Path,
    synrepo_dir: &Path,
    config: &Config,
    generate_new: bool,
    regenerate_stale: bool,
    generator: &dyn CrossLinkGenerator,
) -> crate::Result<GenerationOutcome> {
    if !generate_new && !regenerate_stale {
        return Ok(GenerationOutcome::default());
    }

    let graph = SqliteGraphStore::open_existing(&synrepo_dir.join("graph"))?;
    let mut overlay = SqliteOverlayStore::open(&synrepo_dir.join("overlay"))?;

    let eligible_pairs = select_generation_pairs(
        &graph,
        &overlay,
        synrepo_dir,
        config,
        generate_new,
        regenerate_stale,
    )?;
    if eligible_pairs.is_empty() {
        return Ok(GenerationOutcome::default());
    }

    let blocked_pairs = eligible_pairs
        .len()
        .saturating_sub(config.cross_link_cost_limit as usize);
    let selected_pairs = eligible_pairs
        .into_iter()
        .take(config.cross_link_cost_limit as usize)
        .collect::<Vec<_>>();
    if selected_pairs.is_empty() {
        return Ok(GenerationOutcome {
            inserted: 0,
            blocked_pairs,
        });
    }

    let graph_distances = selected_pairs
        .iter()
        .map(|pair| {
            (
                candidate_key(pair.from, pair.to, pair.kind),
                pair.graph_distance,
            )
        })
        .collect::<HashMap<_, _>>();
    let generated = generator.generate_candidates(&CandidateScope {
        pairs: selected_pairs,
    })?;

    let mut inserted = 0usize;
    for candidate in generated {
        let Some(graph_distance) = graph_distances
            .get(&candidate_key(candidate.from, candidate.to, candidate.kind))
            .copied()
        else {
            continue;
        };

        let Some(verified) =
            verify_candidate(&graph, repo_root, candidate, graph_distance, config)?
        else {
            continue;
        };
        overlay.insert_link(verified)?;
        inserted += 1;
    }

    Ok(GenerationOutcome {
        inserted,
        blocked_pairs,
    })
}

fn select_generation_pairs(
    graph: &SqliteGraphStore,
    overlay: &SqliteOverlayStore,
    _synrepo_dir: &Path,
    _config: &Config,
    generate_new: bool,
    regenerate_stale: bool,
) -> crate::Result<Vec<CandidatePair>> {
    use crate::pipeline::explain::cross_link::triage::{candidate_pairs, TriageScope};

    let concepts = graph
        .all_concept_paths()?
        .into_iter()
        .map(|(_, id)| NodeId::Concept(id))
        .collect::<Vec<_>>();

    // First run deterministic prefilter
    #[allow(unused_mut)]
    let mut pairs = candidate_pairs(
        graph,
        &TriageScope {
            concepts: concepts.clone(),
            ..TriageScope::default()
        },
    )?;

    // If semantic triage is enabled, also run semantic prefilter on unmatched concepts
    #[cfg(feature = "semantic-triage")]
    {
        use crate::pipeline::explain::cross_link::triage::semantic_candidates;

        if _config.enable_semantic_triage {
            // Find concepts that were matched by deterministic
            let matched_concepts: std::collections::HashSet<crate::core::ids::ConceptNodeId> =
                pairs
                    .iter()
                    .filter_map(|p| {
                        if let NodeId::Concept(cid) = p.from {
                            Some(cid)
                        } else {
                            None
                        }
                    })
                    .collect();

            // Get unmatched concepts
            let unmatched_concepts: Vec<NodeId> = concepts
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

            // Run semantic prefilter on unmatched concepts
            if !unmatched_concepts.is_empty() {
                // Load embedding index for query-time embedding
                if let Some(index) = crate::substrate::load_embedding_index(_config, _synrepo_dir)?
                {
                    let semantic_scope = TriageScope {
                        concepts: unmatched_concepts,
                        ..TriageScope::default()
                    };
                    let semantic_pairs =
                        semantic_candidates(graph, &index, _config, &semantic_scope)?;
                    pairs.extend(semantic_pairs);
                }
            }
        }
    }

    if pairs.is_empty() {
        return Ok(Vec::new());
    }

    let existing = overlay
        .all_candidates(None)?
        .into_iter()
        .map(|candidate| {
            (
                candidate_key(candidate.from, candidate.to, candidate.kind),
                candidate,
            )
        })
        .collect::<HashMap<_, _>>();

    let mut selected = Vec::new();
    for pair in pairs {
        match existing.get(&candidate_key(pair.from, pair.to, pair.kind)) {
            None if generate_new => selected.push(pair),
            Some(candidate) if regenerate_stale => {
                let freshness = derive_link_freshness(
                    candidate,
                    current_endpoint_hash(graph, pair.from)?.as_deref(),
                    current_endpoint_hash(graph, pair.to)?.as_deref(),
                );
                if freshness != CrossLinkFreshness::Fresh {
                    selected.push(pair);
                }
            }
            _ => {}
        }
    }

    Ok(selected)
}

fn verify_candidate(
    graph: &SqliteGraphStore,
    repo_root: &Path,
    mut candidate: OverlayLink,
    graph_distance: u32,
    config: &Config,
) -> crate::Result<Option<OverlayLink>> {
    let Some(verified_payload) = verify_candidate_payload(graph, repo_root, &candidate)? else {
        return Ok(None);
    };

    let verified_spans = verified_payload
        .source_spans
        .iter()
        .chain(verified_payload.target_spans.iter())
        .cloned()
        .collect::<Vec<_>>();
    let (score_value, tier) = score(
        &verified_spans,
        graph_distance,
        config.cross_link_confidence_thresholds.into(),
    );

    candidate.source_spans = verified_payload.source_spans;
    candidate.target_spans = verified_payload.target_spans;
    candidate.from_content_hash = verified_payload.from_hash;
    candidate.to_content_hash = verified_payload.to_hash;
    candidate.confidence_score = score_value;
    candidate.confidence_tier = tier;
    candidate.epistemic = match tier {
        ConfidenceTier::High => OverlayEpistemic::MachineAuthoredHighConf,
        ConfidenceTier::ReviewQueue | ConfidenceTier::BelowThreshold => {
            OverlayEpistemic::MachineAuthoredLowConf
        }
    };
    Ok(Some(candidate))
}

fn candidate_key(from: NodeId, to: NodeId, kind: crate::overlay::OverlayEdgeKind) -> String {
    let kind = match kind {
        crate::overlay::OverlayEdgeKind::References => "references",
        crate::overlay::OverlayEdgeKind::Governs => "governs",
        crate::overlay::OverlayEdgeKind::DerivedFrom => "derived_from",
        crate::overlay::OverlayEdgeKind::Mentions => "mentions",
    };
    format!("{from}::{to}::{kind}")
}
