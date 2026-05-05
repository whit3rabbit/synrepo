//! Cross-link candidate generation.
//!
//! Mirrors [`super::CommentaryGenerator`]: a narrow, LLM-agnostic boundary
//! between the explain pipeline and the model that extracts evidence-backed
//! proposed overlay links. Real generation is expensive, so candidate
//! generation always runs behind a deterministic prefilter
//! (`triage::candidate_pairs`) that bounds the work by repo size rather than
//! LLM throughput. Confidence scoring is deterministic too; the LLM only
//! produces the spans and the tier is derived from them.
//!
//! Optional semantic prefilter uses embedding similarity to catch pairs the
//! deterministic prefilter missed.

pub mod ranker;
pub mod triage;

use crate::core::ids::NodeId;
use crate::overlay::{
    CitedSpan, ConfidenceThresholds, ConfidenceTier, OverlayEdgeKind, OverlayLink,
};

/// Source of a candidate pair during triage.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum TriageSource {
    /// Determined via name/identifier token overlap (primary prefilter).
    #[default]
    Deterministic,
    /// Rescued via embedding similarity (semantic prefilter).
    Semantic,
}

impl TriageSource {
    /// User-facing identifier.
    pub fn as_str(&self) -> &'static str {
        match self {
            TriageSource::Deterministic => "deterministic",
            TriageSource::Semantic => "semantic_triage",
        }
    }
}

/// Scope passed to a [`CrossLinkGenerator`]. Carries the candidate pairs the
/// prefilter has already surfaced, so the generator does not traverse the
/// graph itself. The callers (`synrepo sync --generate-cross-links`) are
/// responsible for producing the scope via [`triage::candidate_pairs`].
#[derive(Clone, Debug)]
pub struct CandidateScope {
    /// Prefiltered `(from, to, kind, graph_distance)` tuples awaiting
    /// evidence extraction. `graph_distance` is the shortest hop count
    /// between endpoints over observed edges; the scoring function uses it
    /// as a downweight.
    pub pairs: Vec<CandidatePair>,
}

/// A single prefiltered pair the generator should attempt to verify.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CandidatePair {
    /// Source endpoint (typically a concept or file).
    pub from: NodeId,
    /// Target endpoint (typically a symbol or file).
    pub to: NodeId,
    /// Proposed overlay edge kind.
    pub kind: OverlayEdgeKind,
    /// Graph distance between the endpoints at triage time, in number of
    /// observed edges. `0` means the endpoints share an incident edge;
    /// `u32::MAX` means unreachable (still allowed if name match is strong).
    pub graph_distance: u32,
    /// How this pair was surfaced.
    pub source: TriageSource,
}

/// LLM-agnostic boundary between the explain pipeline and a cross-link
/// proposer.
///
/// Returned links MUST carry non-empty source and target spans with verified
/// LCS ratios; the caller still runs `validate_link` through the storage
/// layer before persisting. Returning `Ok(vec![])` means "the pass saw no
/// evidence worth storing" and is not an error.
pub trait CrossLinkGenerator: Send + Sync {
    /// Produce zero or more evidence-backed overlay link candidates for the
    /// supplied scope.
    fn generate_candidates(&self, scope: &CandidateScope) -> crate::Result<Vec<OverlayLink>>;
}

/// A generator that never produces a candidate. Used as the default when
/// `SYNREPO_ANTHROPIC_API_KEY` is not set so the CLI can still exercise the
/// full persistence and repair pipeline without a live model.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoOpCrossLinkGenerator;

impl CrossLinkGenerator for NoOpCrossLinkGenerator {
    fn generate_candidates(&self, _scope: &CandidateScope) -> crate::Result<Vec<OverlayLink>> {
        Ok(Vec::new())
    }
}

/// Minimum LCS ratio a span must hit for the scoring function to count it
/// toward the `High` tier. Below this threshold, a span still contributes to
/// the review-queue score but never pushes a candidate into `High`.
pub const HIGH_TIER_LCS_FLOOR: f32 = 0.95;

/// Minimum number of verified spans across both endpoints required to reach
/// the `High` tier. Single-span candidates may surface in the review queue
/// but never as `High`.
pub const HIGH_TIER_MIN_SPANS: usize = 2;

/// Graph-distance cutoff above which a candidate cannot reach the `High`
/// tier. Matches the triage cutoff.
pub const HIGH_TIER_MAX_DISTANCE: u32 = 2;

/// Deterministic confidence scoring.
///
/// Combines span count, LCS ratio per span, average span length, and graph
/// distance into a numeric score in `[0.0, 1.0]`, then classifies via
/// [`classify_confidence`]. Pure function so the generation pipeline and the
/// `revalidate_links` repair action both produce the same tier from the same
/// inputs.
///
/// Scoring shape:
/// - Base: mean LCS ratio across all spans, weighted 0.7.
/// - Span-count bonus: +0.2 when at least [`HIGH_TIER_MIN_SPANS`] spans
///   verify; +0.1 for a single span; 0 for none.
/// - Length bonus: up to +0.1, linear in the average normalized length
///   capped at 64 chars.
/// - Distance penalty: -0.1 per hop beyond [`HIGH_TIER_MAX_DISTANCE`], capped
///   at -0.3.
///
/// The positive terms sum to 1.0 so the raw score fills the [0.0, 1.0]
/// partition before clamping.
///
/// The tier is `High` only when all of:
/// - every span's `lcs_ratio` >= [`HIGH_TIER_LCS_FLOOR`]
/// - total span count >= [`HIGH_TIER_MIN_SPANS`]
/// - `graph_distance` <= [`HIGH_TIER_MAX_DISTANCE`]
/// - raw score >= `thresholds.high`
///
/// Missing any of the four drops the candidate to whatever the threshold
/// partition gives it (typically `ReviewQueue` or `BelowThreshold`).
pub fn score(
    spans: &[CitedSpan],
    graph_distance: u32,
    thresholds: ConfidenceThresholds,
) -> (f32, ConfidenceTier) {
    ranker::score_with_features(
        ranker::RankFeatures::extract(spans, graph_distance),
        thresholds,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ids::{ConceptNodeId, SymbolNodeId};

    fn span(lcs: f32, text: &str) -> CitedSpan {
        CitedSpan {
            artifact: NodeId::Concept(ConceptNodeId(1)),
            normalized_text: text.to_string(),
            verified_at_offset: 0,
            lcs_ratio: lcs,
        }
    }

    #[test]
    fn noop_generator_returns_empty() {
        let scope = CandidateScope {
            pairs: vec![CandidatePair {
                from: NodeId::Concept(ConceptNodeId(1)),
                to: NodeId::Symbol(SymbolNodeId(2)),
                kind: OverlayEdgeKind::References,
                graph_distance: 1,
                source: TriageSource::Deterministic,
            }],
        };
        let out = NoOpCrossLinkGenerator.generate_candidates(&scope).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn score_returns_high_only_when_all_criteria_hold() {
        let t = ConfidenceThresholds::default();
        let strong_spans = vec![
            span(0.98, "fn authenticate(user: &User)"),
            span(0.97, "authenticate the user"),
        ];
        let (_, tier) = score(&strong_spans, 1, t);
        assert_eq!(tier, ConfidenceTier::High);

        // One span below the LCS floor → downgrade even with high score.
        let mixed = vec![
            span(0.98, "fn authenticate(user: &User)"),
            span(0.9, "authenticate the user"),
        ];
        let (_, tier) = score(&mixed, 1, t);
        assert_ne!(tier, ConfidenceTier::High);

        // Single span never reaches High.
        let single = vec![span(0.99, "fn authenticate(user: &User)")];
        let (_, tier) = score(&single, 1, t);
        assert_ne!(tier, ConfidenceTier::High);

        // Distance beyond the cutoff drops out of High.
        let far = vec![
            span(0.98, "fn authenticate(user: &User)"),
            span(0.97, "authenticate the user"),
        ];
        let (_, tier) = score(&far, 5, t);
        assert_ne!(tier, ConfidenceTier::High);
    }

    #[test]
    fn score_yields_below_threshold_for_empty_spans() {
        let t = ConfidenceThresholds::default();
        let (raw, tier) = score(&[], 0, t);
        assert_eq!(raw, 0.0);
        assert_eq!(tier, ConfidenceTier::BelowThreshold);
    }
}
