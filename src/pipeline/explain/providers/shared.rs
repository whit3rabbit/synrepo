//! Shared types and helpers used by all explain providers.

use time::OffsetDateTime;

use serde::Deserialize;

use crate::core::ids::NodeId;
use crate::overlay::{
    CitedSpan, ConfidenceThresholds, ConfidenceTier, CrossLinkProvenance, OverlayEdgeKind,
    OverlayEpistemic, OverlayLink,
};
pub use crate::pipeline::explain::commentary_template::{
    has_required_sections, COMMENTARY_MAX_OUTPUT_TOKENS, COMMENTARY_SYSTEM_PROMPT,
};
use crate::pipeline::explain::cross_link::{score, CandidatePair, CandidateScope};

/// System prompt for cross-link evidence generation.
pub const CROSS_LINK_SYSTEM_PROMPT: &str =
    "Propose cross-link evidence between a prose artifact and a code \
     symbol. Return strict JSON only. Never fabricate spans.";

/// Build the user prompt for a cross-link candidate pair.
pub fn cross_link_user_prompt(pair: &CandidatePair) -> String {
    format!(
        "Candidate pair:\n  from: {from}\n  to: {to}\n  relationship: {kind}\n\n\
         Return a JSON object with two fields `source_spans` and \
         `target_spans`, each a list of objects {{ normalized_text, lcs_ratio }}. \
         Only return spans you are confident appear verbatim (modulo whitespace \
         normalization) in the corresponding artifact. An empty list means no evidence.",
        from = pair.from,
        to = pair.to,
        kind = overlay_edge_kind_label(pair.kind),
    )
}

/// Map an overlay edge kind to its display label.
pub fn overlay_edge_kind_label(kind: OverlayEdgeKind) -> &'static str {
    match kind {
        OverlayEdgeKind::References => "references",
        OverlayEdgeKind::Governs => "governs",
        OverlayEdgeKind::DerivedFrom => "derived_from",
        OverlayEdgeKind::Mentions => "mentions",
    }
}

/// Convert a raw span into a cited span attached to an artifact.
pub fn span_into_cited(artifact: NodeId, raw: RawSpan) -> CitedSpan {
    CitedSpan {
        artifact,
        normalized_text: raw.normalized_text,
        verified_at_offset: 0,
        lcs_ratio: raw.lcs_ratio.clamp(0.0, 1.0),
    }
}

/// Parse span JSON from a cross-link response into cited spans.
pub fn parse_spans_from_text(
    text: &str,
    from: NodeId,
    to: NodeId,
) -> Option<(Vec<CitedSpan>, Vec<CitedSpan>)> {
    let spans: SpanPayload = match serde_json::from_str(text.trim()) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "cross-link response was not valid JSON");
            return None;
        }
    };
    let source_spans = spans
        .source_spans
        .into_iter()
        .map(|s| span_into_cited(from, s))
        .collect();
    let target_spans = spans
        .target_spans
        .into_iter()
        .map(|s| span_into_cited(to, s))
        .collect();
    Some((source_spans, target_spans))
}

/// Build overlay links from a cross-link scope by calling `request_spans` for each pair.
///
/// This extracts the `generate_candidates` body shared by all providers.
pub fn build_overlay_links(
    scope: &CandidateScope,
    thresholds: ConfidenceThresholds,
    cross_link_pass_id: &str,
    model: &str,
    request_spans: impl Fn(&CandidatePair) -> Option<(Vec<CitedSpan>, Vec<CitedSpan>)>,
) -> Vec<OverlayLink> {
    let mut out = Vec::new();
    let now = OffsetDateTime::now_utc();
    for pair in &scope.pairs {
        let Some((source_spans, target_spans)) = request_spans(pair) else {
            continue;
        };
        if source_spans.is_empty() || target_spans.is_empty() {
            continue;
        }
        let all_spans: Vec<CitedSpan> = source_spans
            .iter()
            .chain(target_spans.iter())
            .cloned()
            .collect();
        let (score_value, tier) = score(&all_spans, pair.graph_distance, thresholds);
        let epistemic = match tier {
            ConfidenceTier::High => OverlayEpistemic::MachineAuthoredHighConf,
            _ => OverlayEpistemic::MachineAuthoredLowConf,
        };

        out.push(OverlayLink {
            from: pair.from,
            to: pair.to,
            kind: pair.kind,
            epistemic,
            source_spans,
            target_spans,
            from_content_hash: String::new(),
            to_content_hash: String::new(),
            confidence_score: score_value,
            confidence_tier: tier,
            rationale: None,
            provenance: CrossLinkProvenance {
                pass_id: cross_link_pass_id.to_string(),
                model_identity: model.to_string(),
                generated_at: now,
            },
        });
    }
    out
}

/// Cross-link span payload returned by LLM providers.
#[derive(Deserialize)]
pub struct SpanPayload {
    /// Spans found in the source artifact.
    #[serde(default)]
    pub source_spans: Vec<RawSpan>,
    /// Spans found in the target artifact.
    #[serde(default)]
    pub target_spans: Vec<RawSpan>,
}

/// A single span with normalized text and similarity ratio.
#[derive(Deserialize)]
pub struct RawSpan {
    /// Whitespace-normalized text of the span.
    pub normalized_text: String,
    /// Longest-common-subsequence ratio (0.0-1.0).
    #[serde(default = "default_lcs")]
    pub lcs_ratio: f32,
}

/// Default LCS ratio for spans that omit it.
pub fn default_lcs() -> f32 {
    1.0
}

/// Remove provider-visible reasoning blocks and normalize commentary text.
pub fn sanitize_commentary_text(raw: &str) -> String {
    let mut text = raw.trim().to_string();
    loop {
        // ASCII-only folding preserves byte indices for replace_range below.
        let lower = text.to_ascii_lowercase();
        let Some(start) = lower.find("<think>") else {
            break;
        };
        let Some(end_rel) = lower[start..].find("</think>") else {
            text.replace_range(start.., "");
            break;
        };
        let end = start + end_rel + "</think>".len();
        text.replace_range(start..end, "");
    }
    text = strip_markdown_fence(&text);
    text = strip_model_preamble(&text);
    text = strip_markdown_fence_markers(&text);
    text.trim().trim_matches('`').trim().to_string()
}

/// Sanitize model output and reject incomplete structured commentary.
pub fn sanitize_generated_commentary_text(raw: &str) -> Option<String> {
    let text = sanitize_commentary_text(raw);
    if text.is_empty() || !has_required_sections(&text) {
        return None;
    }
    Some(text)
}

fn strip_markdown_fence(raw: &str) -> String {
    let text = raw.trim();
    if !text.starts_with("```") {
        return text.to_string();
    }
    let mut lines = text.lines();
    let _opening = lines.next();
    let mut body: Vec<&str> = lines.collect();
    if body.last().is_some_and(|line| line.trim() == "```") {
        body.pop();
    }
    body.join("\n").trim().to_string()
}

fn strip_markdown_fence_markers(raw: &str) -> String {
    raw.lines()
        .filter(|line| !line.trim_start().starts_with("```"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn strip_model_preamble(raw: &str) -> String {
    let text = raw.trim_start();
    for prefix in [
        "Here is the commentary:",
        "Here is the Markdown commentary:",
        "Here is the advisory commentary:",
        "Advisory commentary:",
    ] {
        if let Some(rest) = text.strip_prefix(prefix) {
            return rest.trim_start().to_string();
        }
    }
    text.to_string()
}

#[cfg(test)]
mod tests {
    use super::{sanitize_commentary_text, sanitize_generated_commentary_text};

    #[test]
    fn sanitize_commentary_text_strips_think_blocks() {
        let text = sanitize_commentary_text("<think>hidden</think>\nUseful commentary.");
        assert_eq!(text, "Useful commentary.");
    }

    #[test]
    fn sanitize_commentary_text_strips_unclosed_think_block() {
        let text = sanitize_commentary_text("Visible.\n<think>hidden");
        assert_eq!(text, "Visible.");
    }

    #[test]
    fn sanitize_commentary_text_strips_markdown_fence() {
        let text = sanitize_commentary_text("```markdown\n## Purpose\nUseful.\n```");
        assert_eq!(text, "## Purpose\nUseful.");
    }

    #[test]
    fn sanitize_commentary_text_strips_internal_fence_markers() {
        let text = sanitize_commentary_text("## Purpose\n```rust\nlet value = 1;\n```\nDone.");
        assert_eq!(text, "## Purpose\nlet value = 1;\nDone.");
    }

    #[test]
    fn sanitize_commentary_text_strips_common_preamble() {
        let text = sanitize_commentary_text("Here is the commentary:\n## Purpose\nUseful.");
        assert_eq!(text, "## Purpose\nUseful.");
    }

    #[test]
    fn sanitize_generated_commentary_text_rejects_partial_template() {
        assert!(sanitize_generated_commentary_text("## Purpose\nOnly one section.").is_none());
    }
}
