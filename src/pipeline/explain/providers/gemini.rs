//! Google Gemini explain provider.
//!
//! Calls the Gemini generateContent API using a blocking `reqwest::Client`. API key is read from
//! `GEMINI_API_KEY`. Model can be overridden via `SYNREPO_LLM_MODEL`.

use time::OffsetDateTime;

use crate::core::ids::NodeId;
use crate::overlay::{CitedSpan, ConfidenceThresholds, OverlayLink};
use crate::pipeline::explain::cross_link::{CandidatePair, CandidateScope};
use crate::pipeline::explain::telemetry::{publish_budget_blocked, CallCtx, ExplainTarget};
use crate::pipeline::explain::{CommentaryEntry, CommentaryGenerator, CrossLinkGenerator};

use super::http::{
    build_client, cap_output_bytes, estimate_tokens, post_json_strict, resolve_usage,
    UsageResolution,
};
use super::shared::*;

const PROVIDER: &str = "gemini";

/// Default Gemini model for explain (gemini-1.5-flash is fast and capable).
pub const DEFAULT_MODEL: &str = "gemini-1.5-flash";

const API_URL_BASE: &str = "https://generativelanguage.googleapis.com/v1beta/models";
const PASS_ID: &str = "commentary-v1-gemini";
const CROSS_LINK_PASS_ID: &str = "cross-link-v1-gemini";

/// Build the full API URL for a given model.
fn api_url(model: &str) -> String {
    format!("{}/{model}:generateContent", API_URL_BASE)
}

fn count_tokens_url(model: &str) -> String {
    format!("{}/{model}:countTokens", API_URL_BASE)
}

/// Gemini-backed commentary generator.
pub struct GeminiCommentaryGenerator {
    api_key: String,
    model: String,
    max_tokens_per_call: u32,
    client: reqwest::blocking::Client,
}

impl GeminiCommentaryGenerator {
    /// Construct a generator with an explicit API key.
    pub fn new(api_key: String, model: String, max_tokens_per_call: u32) -> Self {
        let client = build_client();
        Self {
            api_key,
            model,
            max_tokens_per_call,
            client,
        }
    }
}

impl CommentaryGenerator for GeminiCommentaryGenerator {
    fn generate(&self, node: NodeId, context: &str) -> crate::Result<Option<CommentaryEntry>> {
        let target = ExplainTarget::Commentary { node };
        let count_request = GeminiCountTokensRequest {
            contents: vec![Content {
                role: "user",
                parts: vec![Part { text: context }],
            }],
            system_instruction: Some(Instruction {
                role: "system",
                parts: vec![Part {
                    text: COMMENTARY_SYSTEM_PROMPT,
                }],
            }),
        };

        let estimated_tokens =
            count_input_tokens(&self.client, &self.api_key, &self.model, &count_request)
                .unwrap_or_else(|| estimate_tokens(context));
        if estimated_tokens > self.max_tokens_per_call {
            publish_budget_blocked(
                PROVIDER,
                &self.model,
                target,
                estimated_tokens,
                self.max_tokens_per_call,
            );
            return Ok(None);
        }

        let body = GeminiRequest {
            contents: vec![Content {
                role: "user",
                parts: vec![Part { text: context }],
            }],
            system_instruction: Some(Instruction {
                role: "system",
                parts: vec![Part {
                    text: COMMENTARY_SYSTEM_PROMPT,
                }],
            }),
            generation_config: GenerationConfig {
                max_output_tokens: 512,
                temperature: Some(0.7),
            },
        };

        let url = format!("{}?key={}", api_url(&self.model), self.api_key);
        let headers = [("Content-Type", "application/json")];

        let ctx = CallCtx::start(PROVIDER, &self.model, target);
        let parsed: GeminiResponse = match post_json_strict(&self.client, &url, &headers, &body) {
            Ok(p) => p,
            Err(e) => {
                ctx.fail(e);
                return Ok(None);
            }
        };

        let text = parsed
            .candidates
            .into_iter()
            .next()
            .and_then(|c| c.content)
            .and_then(|content| content.parts.into_iter().next().map(|p| p.text))
            .map(|s| s.trim().to_string())
            .unwrap_or_default();

        let usage = resolve_usage(UsageResolution::from_output_text(
            parsed
                .usage_metadata
                .as_ref()
                .map(|u| (u.prompt_token_count, u.candidates_token_count)),
            estimated_tokens,
            &text,
        ));
        ctx.complete(usage, cap_output_bytes(&text));

        if text.is_empty() {
            return Ok(None);
        }

        Ok(Some(CommentaryEntry {
            node_id: node,
            text,
            provenance: crate::overlay::CommentaryProvenance {
                source_content_hash: String::new(),
                pass_id: PASS_ID.to_string(),
                model_identity: self.model.clone(),
                generated_at: OffsetDateTime::now_utc(),
            },
        }))
    }
}

/// Gemini-backed cross-link generator.
pub struct GeminiCrossLinkGenerator {
    api_key: String,
    model: String,
    max_tokens_per_call: u32,
    thresholds: ConfidenceThresholds,
    client: reqwest::blocking::Client,
}

impl GeminiCrossLinkGenerator {
    /// Construct a generator with an explicit API key.
    pub fn new(
        api_key: String,
        model: String,
        max_tokens_per_call: u32,
        thresholds: ConfidenceThresholds,
    ) -> Self {
        let client = build_client();
        Self {
            api_key,
            model,
            max_tokens_per_call,
            thresholds,
            client,
        }
    }

    fn request_spans(&self, pair: &CandidatePair) -> Option<(Vec<CitedSpan>, Vec<CitedSpan>)> {
        let prompt = cross_link_user_prompt(pair);

        let target = ExplainTarget::CrossLink {
            from: pair.from,
            to: pair.to,
            kind: pair.kind,
        };
        let count_request = GeminiCountTokensRequest {
            contents: vec![Content {
                role: "user",
                parts: vec![Part { text: &prompt }],
            }],
            system_instruction: Some(Instruction {
                role: "system",
                parts: vec![Part {
                    text: CROSS_LINK_SYSTEM_PROMPT,
                }],
            }),
        };

        let estimated_tokens =
            count_input_tokens(&self.client, &self.api_key, &self.model, &count_request)
                .unwrap_or_else(|| estimate_tokens(&prompt));
        if estimated_tokens > self.max_tokens_per_call {
            publish_budget_blocked(
                PROVIDER,
                &self.model,
                target,
                estimated_tokens,
                self.max_tokens_per_call,
            );
            return None;
        }

        let body = GeminiRequest {
            contents: vec![Content {
                role: "user",
                parts: vec![Part { text: &prompt }],
            }],
            system_instruction: Some(Instruction {
                role: "system",
                parts: vec![Part {
                    text: CROSS_LINK_SYSTEM_PROMPT,
                }],
            }),
            generation_config: GenerationConfig {
                max_output_tokens: 512,
                temperature: Some(0.7),
            },
        };

        let url = format!("{}?key={}", api_url(&self.model), self.api_key);
        let headers = [("Content-Type", "application/json")];

        let ctx = CallCtx::start(PROVIDER, &self.model, target);
        let parsed: GeminiResponse = match post_json_strict(&self.client, &url, &headers, &body) {
            Ok(p) => p,
            Err(e) => {
                ctx.fail(e);
                return None;
            }
        };

        let text = parsed
            .candidates
            .into_iter()
            .next()
            .and_then(|c| c.content)
            .and_then(|content| content.parts.into_iter().next().map(|p| p.text))
            .unwrap_or_default();

        let usage = resolve_usage(UsageResolution::from_output_text(
            parsed
                .usage_metadata
                .as_ref()
                .map(|u| (u.prompt_token_count, u.candidates_token_count)),
            estimated_tokens,
            &text,
        ));
        ctx.complete(usage, cap_output_bytes(&text));

        parse_spans_from_text(&text, pair.from, pair.to)
    }
}

impl CrossLinkGenerator for GeminiCrossLinkGenerator {
    fn generate_candidates(&self, scope: &CandidateScope) -> crate::Result<Vec<OverlayLink>> {
        Ok(build_overlay_links(
            scope,
            self.thresholds,
            CROSS_LINK_PASS_ID,
            &self.model,
            |pair| self.request_spans(pair),
        ))
    }
}

// Gemini-specific request/response types

use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct GeminiRequest<'a> {
    contents: Vec<Content<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<Instruction<'a>>,
    generation_config: GenerationConfig,
}

#[derive(Serialize)]
struct GeminiCountTokensRequest<'a> {
    contents: Vec<Content<'a>>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "systemInstruction")]
    system_instruction: Option<Instruction<'a>>,
}

#[derive(Serialize)]
struct Content<'a> {
    role: &'a str,
    parts: Vec<Part<'a>>,
}

#[derive(Serialize)]
struct Instruction<'a> {
    role: &'a str,
    parts: Vec<Part<'a>>,
}

#[derive(Serialize)]
struct Part<'a> {
    text: &'a str,
}

#[derive(Serialize, Default)]
struct GenerationConfig {
    max_output_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Deserialize)]
struct GeminiResponse {
    candidates: Vec<Candidate>,
    #[serde(default, rename = "usageMetadata")]
    usage_metadata: Option<GeminiUsage>,
}

#[derive(Deserialize)]
struct GeminiUsage {
    #[serde(default, rename = "promptTokenCount")]
    prompt_token_count: u32,
    #[serde(default, rename = "candidatesTokenCount")]
    candidates_token_count: u32,
}

#[derive(Deserialize)]
struct GeminiCountTokensResponse {
    #[serde(default, rename = "totalTokens")]
    total_tokens: u32,
}

#[derive(Deserialize)]
struct Candidate {
    content: Option<ContentResponse>,
}

#[derive(Deserialize)]
struct ContentResponse {
    parts: Vec<PartResponse>,
}

#[derive(Deserialize)]
struct PartResponse {
    text: String,
}

fn count_input_tokens(
    client: &reqwest::blocking::Client,
    api_key: &str,
    model: &str,
    request: &GeminiCountTokensRequest<'_>,
) -> Option<u32> {
    let url = format!("{}?key={}", count_tokens_url(model), api_key);
    let headers = [("Content-Type", "application/json")];
    post_json_strict::<GeminiCountTokensRequest<'_>, GeminiCountTokensResponse>(
        client, &url, &headers, request,
    )
    .ok()
    .map(|response| response.total_tokens)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ids::SymbolNodeId;

    #[test]
    fn new_constructs_without_panicking() {
        let gen =
            GeminiCommentaryGenerator::new("fake-key".to_string(), "test-model".to_string(), 5000);
        let node = NodeId::Symbol(SymbolNodeId(1));
        // This will fail (no API key) but shouldn't panic
        let _ = gen.generate(node, "context");
    }

    #[test]
    fn oversized_context_skips_generation() {
        let context = "x".repeat(50_000);
        let gen =
            GeminiCommentaryGenerator::new("fake-key".to_string(), "test-model".to_string(), 5000);
        let node = NodeId::Symbol(SymbolNodeId(1));
        let entry = gen.generate(node, &context).unwrap();
        assert!(entry.is_none(), "oversized context must skip generation");
    }
}
