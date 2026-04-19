use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use crate::{
    core::ids::NodeId,
    overlay::{
        with_overlay_read_snapshot, ConfidenceTier, CrossLinkFreshness, CrossLinkProvenance,
        OverlayEdgeKind,
    },
    structure::graph::GraphReader,
};

use super::SqliteOverlayStore;

/// Query filters for operator-facing cross-link findings.
#[derive(Clone, Debug, Default)]
pub struct FindingsFilter {
    /// Restrict findings to candidates touching this node.
    pub node_id: Option<NodeId>,
    /// Restrict findings to a specific overlay edge kind.
    pub kind: Option<OverlayEdgeKind>,
    /// Restrict findings to a specific freshness state.
    pub freshness: Option<CrossLinkFreshness>,
    /// Maximum number of findings to return.
    pub limit: Option<usize>,
}

/// Operator-facing cross-link finding surfaced by CLI and MCP audit paths.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CrossLinkFinding {
    /// Stable CLI/MCP identifier for the candidate triple.
    pub candidate_id: String,
    /// Source endpoint node ID in display form.
    pub from_node_id: String,
    /// Target endpoint node ID in display form.
    pub to_node_id: String,
    /// Proposed relationship kind.
    pub kind: OverlayEdgeKind,
    /// Surfaced confidence tier.
    pub tier: ConfidenceTier,
    /// Numeric confidence score retained for audit and threshold tuning.
    pub score: f32,
    /// Current freshness relative to the graph.
    pub freshness: CrossLinkFreshness,
    /// Number of verified spans cited from the source endpoint.
    pub source_span_count: usize,
    /// Number of verified spans cited from the target endpoint.
    pub target_span_count: usize,
    /// One-line generator rationale, when present.
    pub rationale: Option<String>,
    /// Full generation provenance.
    pub provenance: CrossLinkProvenance,
}

impl SqliteOverlayStore {
    /// Query operator-facing findings over the active cross-link overlay.
    pub fn findings(
        &self,
        graph: &dyn GraphReader,
        filter: &FindingsFilter,
    ) -> crate::Result<Vec<CrossLinkFinding>> {
        with_overlay_read_snapshot(self, |overlay| {
            let candidates = match filter.node_id {
                Some(node_id) => overlay.links_for(node_id)?,
                None => overlay.all_candidates(None)?,
            };

            let mut findings = Vec::new();
            for candidate in candidates {
                if filter.kind.is_some_and(|kind| candidate.kind != kind) {
                    continue;
                }

                let freshness = crate::overlay::derive_link_freshness(
                    &candidate,
                    current_endpoint_hash(graph, candidate.from)?.as_deref(),
                    current_endpoint_hash(graph, candidate.to)?.as_deref(),
                );

                if filter
                    .freshness
                    .is_some_and(|expected| freshness != expected)
                {
                    continue;
                }

                if !matches_default_audit_surface(candidate.confidence_tier, freshness) {
                    continue;
                }

                findings.push(CrossLinkFinding {
                    candidate_id: format_candidate_id(
                        candidate.from,
                        candidate.to,
                        candidate.kind,
                        &candidate.provenance.pass_id,
                    ),
                    from_node_id: candidate.from.to_string(),
                    to_node_id: candidate.to.to_string(),
                    kind: candidate.kind,
                    tier: candidate.confidence_tier,
                    score: candidate.confidence_score,
                    freshness,
                    source_span_count: candidate.source_spans.len(),
                    target_span_count: candidate.target_spans.len(),
                    rationale: candidate.rationale,
                    provenance: candidate.provenance,
                });
            }

            findings.sort_by(compare_findings);
            if let Some(limit) = filter.limit {
                findings.truncate(limit);
            }

            Ok(findings)
        })
    }
}

/// Length of the `pass_id` suffix included in a candidate ID. Binds a
/// reviewed revision to the accept call: when the generator re-runs and
/// produces a new `pass_id`, the ID shown to the reviewer no longer matches
/// the stored row, so stale `links accept` calls fail loudly instead of
/// silently promoting a different revision with the same endpoint triple.
pub const CANDIDATE_ID_PASS_SUFFIX_LEN: usize = 12;

/// Return the stable pass-id suffix for a given provenance `pass_id`.
/// Callers should not truncate `pass_id` inline; use this helper so the
/// format stays consistent with `parse_candidate_id`.
pub fn candidate_pass_suffix(pass_id: &str) -> &str {
    let take = CANDIDATE_ID_PASS_SUFFIX_LEN.min(pass_id.len());
    &pass_id[..take]
}

/// Stable identifier for an overlay candidate revision. Format:
/// `{from}::{to}::{kind}::{pass_suffix}`.
///
/// `pass_id` is the generation-pass identifier from `CrossLinkProvenance`.
/// Including a suffix here makes the ID revision-bound: a second generation
/// pass for the same `(from, to, kind)` triple produces a different ID, so
/// a reviewer who captured the old ID cannot silently promote the new row.
pub fn format_candidate_id(
    from: NodeId,
    to: NodeId,
    kind: OverlayEdgeKind,
    pass_id: &str,
) -> String {
    format!(
        "{}::{}::{}::{}",
        from,
        to,
        overlay_edge_kind_as_str(kind),
        candidate_pass_suffix(pass_id),
    )
}

/// Parse a CLI/MCP freshness filter.
pub fn parse_cross_link_freshness(value: &str) -> crate::Result<CrossLinkFreshness> {
    match value {
        "fresh" => Ok(CrossLinkFreshness::Fresh),
        "stale" => Ok(CrossLinkFreshness::Stale),
        "source_deleted" => Ok(CrossLinkFreshness::SourceDeleted),
        "invalid" => Ok(CrossLinkFreshness::Invalid),
        "missing" => Ok(CrossLinkFreshness::Missing),
        other => Err(crate::Error::Other(anyhow::anyhow!(
            "invalid freshness state: {other}"
        ))),
    }
}

/// Parse a CLI/MCP overlay edge-kind filter.
pub fn parse_overlay_edge_kind(value: &str) -> crate::Result<OverlayEdgeKind> {
    match value {
        "references" => Ok(OverlayEdgeKind::References),
        "governs" => Ok(OverlayEdgeKind::Governs),
        "derived_from" => Ok(OverlayEdgeKind::DerivedFrom),
        "mentions" => Ok(OverlayEdgeKind::Mentions),
        other => Err(crate::Error::Other(anyhow::anyhow!(
            "invalid overlay edge kind: {other}"
        ))),
    }
}

/// Descending NaN-safe `f32` comparison used by every candidate/finding sort.
/// SQLite rejects NaN at NOT NULL boundaries, but in-memory candidate sources
/// can still construct it, so callers must not use `partial_cmp().unwrap()`.
pub fn compare_score_desc(left: f32, right: f32) -> Ordering {
    right.partial_cmp(&left).unwrap_or(Ordering::Equal)
}

fn compare_findings(left: &CrossLinkFinding, right: &CrossLinkFinding) -> Ordering {
    compare_score_desc(left.score, right.score)
        .then_with(|| {
            right
                .provenance
                .generated_at
                .cmp(&left.provenance.generated_at)
        })
        .then_with(|| left.candidate_id.cmp(&right.candidate_id))
}

fn matches_default_audit_surface(tier: ConfidenceTier, freshness: CrossLinkFreshness) -> bool {
    matches!(
        tier,
        ConfidenceTier::ReviewQueue | ConfidenceTier::BelowThreshold
    ) || freshness == CrossLinkFreshness::SourceDeleted
}

fn current_endpoint_hash(graph: &dyn GraphReader, node: NodeId) -> crate::Result<Option<String>> {
    match node {
        NodeId::File(file_id) => Ok(graph.get_file(file_id)?.map(|file| file.content_hash)),
        NodeId::Symbol(symbol_id) => {
            let Some(symbol) = graph.get_symbol(symbol_id)? else {
                return Ok(None);
            };
            Ok(graph
                .get_file(symbol.file_id)?
                .map(|file| file.content_hash))
        }
        NodeId::Concept(concept_id) => {
            let Some(concept) = graph.get_concept(concept_id)? else {
                return Ok(None);
            };
            if let Some(file) = graph.file_by_path(&concept.path)? {
                return Ok(Some(file.content_hash));
            }
            Ok(concept
                .provenance
                .source_artifacts
                .first()
                .map(|source| source.content_hash.clone()))
        }
    }
}

fn overlay_edge_kind_as_str(kind: OverlayEdgeKind) -> &'static str {
    match kind {
        OverlayEdgeKind::References => "references",
        OverlayEdgeKind::Governs => "governs",
        OverlayEdgeKind::DerivedFrom => "derived_from",
        OverlayEdgeKind::Mentions => "mentions",
    }
}
