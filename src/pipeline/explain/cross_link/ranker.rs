//! Cross-link rank features and deterministic scorer.

use crate::overlay::{classify_confidence, CitedSpan, ConfidenceThresholds, ConfidenceTier};

use super::{HIGH_TIER_LCS_FLOOR, HIGH_TIER_MAX_DISTANCE, HIGH_TIER_MIN_SPANS};

const BASE_LCS_WEIGHT: f32 = 0.7;
const MULTI_SPAN_BONUS: f32 = 0.2;
const SINGLE_SPAN_BONUS: f32 = 0.1;
const LENGTH_BONUS: f32 = 0.1;
const OVER_DISTANCE_PENALTY: f32 = 0.1;
const LENGTH_BONUS_CAP_CHARS: usize = 64;
const OVER_DISTANCE_HOP_CAP: u32 = 3;

/// Features used by the v1 ranker. No triage-source field is included because
/// that signal is not persisted and repair revalidation must reproduce scores.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RankFeatures {
    /// Number of verified spans.
    pub span_count: usize,
    /// Mean span LCS ratio.
    pub mean_lcs_ratio: f32,
    /// Average normalized span length, capped at the scorer saturation point.
    pub avg_span_length: f32,
    /// Graph distance in observed hops.
    pub graph_distance: u32,
    /// Whether every span clears the hard high-tier LCS floor.
    pub all_spans_strong: bool,
}

impl RankFeatures {
    /// Extract scorer features from verified spans and graph distance.
    pub fn extract(spans: &[CitedSpan], graph_distance: u32) -> Self {
        if spans.is_empty() {
            return Self {
                span_count: 0,
                mean_lcs_ratio: 0.0,
                avg_span_length: 0.0,
                graph_distance,
                all_spans_strong: false,
            };
        }

        Self {
            span_count: spans.len(),
            mean_lcs_ratio: spans.iter().map(|s| s.lcs_ratio).sum::<f32>() / spans.len() as f32,
            avg_span_length: spans
                .iter()
                .map(|s| s.normalized_text.len().min(LENGTH_BONUS_CAP_CHARS) as f32)
                .sum::<f32>()
                / spans.len() as f32,
            graph_distance,
            all_spans_strong: spans.iter().all(|s| s.lcs_ratio >= HIGH_TIER_LCS_FLOOR),
        }
    }
}

/// Score rank features with coefficients matching the original weighted sum.
pub fn score_with_features(
    features: RankFeatures,
    thresholds: ConfidenceThresholds,
) -> (f32, ConfidenceTier) {
    if features.span_count == 0 {
        return (0.0, ConfidenceTier::BelowThreshold);
    }

    let base = features.mean_lcs_ratio * BASE_LCS_WEIGHT;
    let span_bonus = if features.span_count >= HIGH_TIER_MIN_SPANS {
        MULTI_SPAN_BONUS
    } else {
        SINGLE_SPAN_BONUS
    };
    let length_bonus = (features.avg_span_length / LENGTH_BONUS_CAP_CHARS as f32) * LENGTH_BONUS;
    let distance_penalty = if features.graph_distance <= HIGH_TIER_MAX_DISTANCE {
        0.0
    } else {
        let over =
            (features.graph_distance - HIGH_TIER_MAX_DISTANCE).min(OVER_DISTANCE_HOP_CAP) as f32;
        -over * OVER_DISTANCE_PENALTY
    };

    let raw = (base + span_bonus + length_bonus + distance_penalty).clamp(0.0, 1.0);
    let high_eligible = features.all_spans_strong
        && features.span_count >= HIGH_TIER_MIN_SPANS
        && features.graph_distance <= HIGH_TIER_MAX_DISTANCE;
    let tier = match classify_confidence(raw, thresholds) {
        ConfidenceTier::High if !high_eligible => ConfidenceTier::ReviewQueue,
        t => t,
    };
    (raw, tier)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ids::{ConceptNodeId, NodeId};

    fn span(lcs: f32, text: &str) -> CitedSpan {
        CitedSpan {
            artifact: NodeId::Concept(ConceptNodeId(1)),
            normalized_text: text.to_string(),
            verified_at_offset: 0,
            lcs_ratio: lcs,
        }
    }

    #[test]
    fn empty_features_stay_below_threshold() {
        let (score, tier) = score_with_features(
            RankFeatures::extract(&[], 0),
            ConfidenceThresholds::default(),
        );
        assert_eq!(score, 0.0);
        assert_eq!(tier, ConfidenceTier::BelowThreshold);
    }

    #[test]
    fn far_distance_cannot_reach_high() {
        let spans = [
            span(0.99, "alpha beta gamma"),
            span(0.98, "alpha beta delta"),
        ];
        let (_, tier) = score_with_features(
            RankFeatures::extract(&spans, HIGH_TIER_MAX_DISTANCE + 1),
            ConfidenceThresholds::default(),
        );
        assert_ne!(tier, ConfidenceTier::High);
    }
}
