//! Claude-backed cross-link generator.
//!
//! Mirrors [`super::claude::ClaudeCommentaryGenerator`]: a blocking HTTP
//! client that calls the Claude Messages API when
//! `SYNREPO_ANTHROPIC_API_KEY` is set and returns `NoOpCrossLinkGenerator`
//! otherwise. This is the first live cross-link generation path and is
//! intentionally minimal. Evidence verification (fuzzy LCS) and confidence
//! scoring happen in the caller, not here; the generator's job is to propose
//! spans, not to score them.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::core::ids::NodeId;
use crate::overlay::{
    CitedSpan, ConfidenceThresholds, ConfidenceTier, CrossLinkProvenance, OverlayEdgeKind,
    OverlayEpistemic, OverlayLink,
};

use super::cross_link::{score, CandidatePair, CandidateScope};
use super::{CrossLinkGenerator, NoOpCrossLinkGenerator};

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2023-06-01";
const DEFAULT_MODEL: &str = "claude-sonnet-4-6";
const ENV_KEY: &str = "SYNREPO_ANTHROPIC_API_KEY";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const PASS_ID: &str = "cross-link-v1";

/// Conservative chars-per-token upper bound, same as the commentary generator.
const CHARS_PER_TOKEN: u32 = 4;

/// Live cross-link generator backed by the Claude Messages API.
pub struct ClaudeCrossLinkGenerator {
    api_key: String,
    model: String,
    max_tokens_per_call: u32,
    thresholds: ConfidenceThresholds,
    client: reqwest::blocking::Client,
}

impl ClaudeCrossLinkGenerator {
    /// Construct a generator with an explicit API key.
    pub fn new(
        api_key: String,
        max_tokens_per_call: u32,
        thresholds: ConfidenceThresholds,
    ) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new());
        Self {
            api_key,
            model: DEFAULT_MODEL.to_string(),
            max_tokens_per_call,
            thresholds,
            client,
        }
    }

    /// Construct a live generator when the API key env var is set, else a
    /// [`NoOpCrossLinkGenerator`]. Prefer this over `new` for the common
    /// "ship with graceful degradation" path.
    pub fn new_or_noop(
        max_tokens_per_call: u32,
        thresholds: ConfidenceThresholds,
    ) -> Box<dyn CrossLinkGenerator> {
        match std::env::var(ENV_KEY) {
            Ok(key) if !key.is_empty() => Box::new(Self::new(key, max_tokens_per_call, thresholds)),
            _ => Box::new(NoOpCrossLinkGenerator),
        }
    }

    fn request_spans(&self, pair: &CandidatePair) -> Option<(Vec<CitedSpan>, Vec<CitedSpan>)> {
        // Build a short structured prompt; the caller has already done the
        // prefilter, so the model only sees one pair per call. For the
        // initial live slice the prompt is deliberately minimal — the
        // schema-driven span-extraction path can be added later without
        // changing the trait.
        let prompt = format!(
            "Candidate pair:\n  from: {from}\n  to: {to}\n  relationship: {kind}\n\n\
             Return a JSON object with two fields `source_spans` and \
             `target_spans`, each a list of objects `{{ normalized_text, lcs_ratio }}`. \
             Only return spans you are confident appear verbatim (modulo whitespace \
             normalization) in the corresponding artifact. An empty list means no evidence.",
            from = pair.from,
            to = pair.to,
            kind = overlay_edge_kind_label(pair.kind),
        );

        let estimated_tokens = (prompt.len() as u32) / CHARS_PER_TOKEN;
        if estimated_tokens > self.max_tokens_per_call {
            tracing::warn!(
                estimated = estimated_tokens,
                budget = self.max_tokens_per_call,
                "cross-link generation skipped: context exceeds configured cost limit"
            );
            return None;
        }

        let body = MessagesRequest {
            model: &self.model,
            max_tokens: 512,
            system: "Propose cross-link evidence between a prose artifact and a code \
                     symbol. Return strict JSON only. Never fabricate spans.",
            messages: vec![Message {
                role: "user",
                content: &prompt,
            }],
        };

        let response = match self
            .client
            .post(API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
        {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(error = %e, "cross-link generation request failed");
                return None;
            }
        };
        if !response.status().is_success() {
            tracing::warn!(
                status = %response.status(),
                "cross-link generation returned non-success status"
            );
            return None;
        }

        let parsed: MessagesResponse = match response.json() {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(error = %e, "cross-link response parse failed");
                return None;
            }
        };

        let text = parsed
            .content
            .into_iter()
            .filter(|b| b.ty == "text")
            .map(|b| b.text)
            .collect::<Vec<_>>()
            .join("\n");

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
            .map(|s| span_into_cited(pair.from, s))
            .collect::<Vec<_>>();
        let target_spans = spans
            .target_spans
            .into_iter()
            .map(|s| span_into_cited(pair.to, s))
            .collect::<Vec<_>>();
        Some((source_spans, target_spans))
    }
}

impl CrossLinkGenerator for ClaudeCrossLinkGenerator {
    fn generate_candidates(&self, scope: &CandidateScope) -> crate::Result<Vec<OverlayLink>> {
        let mut out = Vec::new();
        let now = OffsetDateTime::now_utc();
        for pair in &scope.pairs {
            let Some((source_spans, target_spans)) = self.request_spans(pair) else {
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
            let (score_value, tier) = score(&all_spans, pair.graph_distance, self.thresholds);
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
                    pass_id: PASS_ID.to_string(),
                    model_identity: self.model.clone(),
                    generated_at: now,
                },
            });
        }
        Ok(out)
    }
}

fn overlay_edge_kind_label(kind: OverlayEdgeKind) -> &'static str {
    match kind {
        OverlayEdgeKind::References => "references",
        OverlayEdgeKind::Governs => "governs",
        OverlayEdgeKind::DerivedFrom => "derived_from",
        OverlayEdgeKind::Mentions => "mentions",
    }
}

fn span_into_cited(artifact: NodeId, raw: RawSpan) -> CitedSpan {
    CitedSpan {
        artifact,
        normalized_text: raw.normalized_text,
        verified_at_offset: 0,
        lcs_ratio: raw.lcs_ratio.clamp(0.0, 1.0),
    }
}

#[derive(Serialize)]
struct MessagesRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    system: &'a str,
    messages: Vec<Message<'a>>,
}

#[derive(Serialize)]
struct Message<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct MessagesResponse {
    content: Vec<ContentBlock>,
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    ty: String,
    #[serde(default)]
    text: String,
}

#[derive(Deserialize)]
struct SpanPayload {
    #[serde(default)]
    source_spans: Vec<RawSpan>,
    #[serde(default)]
    target_spans: Vec<RawSpan>,
}

#[derive(Deserialize)]
struct RawSpan {
    normalized_text: String,
    #[serde(default = "default_lcs")]
    lcs_ratio: f32,
}

fn default_lcs() -> f32 {
    1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_or_noop_constructs_without_panicking() {
        let gen = ClaudeCrossLinkGenerator::new_or_noop(5000, ConfidenceThresholds::default());
        let _ = gen.generate_candidates(&CandidateScope { pairs: Vec::new() });
    }
}
