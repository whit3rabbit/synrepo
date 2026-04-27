//! Generic OpenAI-compatible chat completion provider.
//!
//! Parameterizes the OpenAI Chat Completions wire format so that providers
//! sharing the same request/response shape (OpenAI, Z.ai, MiniMax, OpenRouter)
//! only need to supply a config struct.

use time::OffsetDateTime;

use serde::{Deserialize, Serialize};

use crate::core::ids::NodeId;
use crate::overlay::{CitedSpan, ConfidenceThresholds};
use crate::pipeline::explain::cross_link::CandidatePair;
use crate::pipeline::explain::telemetry::{publish_budget_blocked, CallCtx, ExplainTarget};
use crate::pipeline::explain::{CommentaryEntry, CommentaryGenerator, CrossLinkGenerator};

use super::http::{
    build_client, cap_output_bytes, estimate_tokens, post_json_strict, resolve_usage,
    UsageResolution,
};
use super::shared::*;

/// Configuration that varies per OpenAI-compatible provider.
pub struct OpenAiCompatConfig {
    /// Provider name for telemetry and logging.
    pub provider: &'static str,
    /// Chat completions API endpoint URL.
    pub api_url: &'static str,
    /// Commentary provenance pass ID.
    pub pass_id: &'static str,
    /// Cross-link provenance pass ID.
    pub cross_link_pass_id: &'static str,
    /// Default model when none is specified.
    pub default_model: &'static str,
    /// Extra HTTP headers (e.g. OpenRouter Referer/X-Title).
    pub extra_headers: &'static [(&'static str, &'static str)],
    /// Optional post-response hook for usage/cost overrides (OpenRouter).
    #[allow(clippy::type_complexity)]
    pub on_response:
        Option<fn(&ChatResponse, &reqwest::blocking::Client, &[(&str, &str)]) -> ResponseExtras>,
}

/// Extra data returned by an optional post-response hook.
#[derive(Default)]
pub struct ResponseExtras {
    /// Override reported (prompt, completion) token counts.
    pub usage_override: Option<(u32, u32)>,
    /// Billed cost in USD, if the provider reports it.
    pub billed_cost: Option<f64>,
}

/// Generic provider for OpenAI-compatible chat completion APIs.
pub struct OpenAiCompatProvider {
    config: &'static OpenAiCompatConfig,
    api_key: String,
    model: String,
    max_tokens_per_call: u32,
    thresholds: Option<ConfidenceThresholds>,
    client: reqwest::blocking::Client,
}

impl OpenAiCompatProvider {
    fn new(
        config: &'static OpenAiCompatConfig,
        api_key: String,
        model: String,
        max_tokens_per_call: u32,
        thresholds: Option<ConfidenceThresholds>,
    ) -> Self {
        Self {
            config,
            api_key,
            model,
            max_tokens_per_call,
            thresholds,
            client: build_client(),
        }
    }

    /// Create a commentary generator.
    pub fn new_commentary(
        config: &'static OpenAiCompatConfig,
        api_key: String,
        model: String,
        max_tokens_per_call: u32,
    ) -> Self {
        Self::new(config, api_key, model, max_tokens_per_call, None)
    }

    /// Create a cross-link generator.
    pub fn new_cross_link(
        config: &'static OpenAiCompatConfig,
        api_key: String,
        model: String,
        max_tokens_per_call: u32,
        thresholds: ConfidenceThresholds,
    ) -> Self {
        Self::new(
            config,
            api_key,
            model,
            max_tokens_per_call,
            Some(thresholds),
        )
    }

    /// Build auth + extra headers for a request.
    fn build_headers<'a>(&self, auth_header: &'a str) -> Vec<(&'a str, &'a str)> {
        let mut headers = vec![
            ("Authorization", auth_header),
            ("Content-Type", "application/json"),
        ];
        for &(k, v) in self.config.extra_headers {
            headers.push((k, v));
        }
        headers
    }

    /// Run the optional post-response hook.
    fn resolve_extras(&self, parsed: &ChatResponse, headers: &[(&str, &str)]) -> ResponseExtras {
        self.config
            .on_response
            .map(|hook| hook(parsed, &self.client, headers))
            .unwrap_or_default()
    }

    /// Resolve usage from the response, applying any hook overrides.
    fn resolve_response_usage(
        &self,
        parsed: &ChatResponse,
        extras: &ResponseExtras,
        estimated_tokens: u32,
        text: &str,
    ) -> (crate::pipeline::explain::telemetry::TokenUsage, Option<f64>) {
        let reported = extras.usage_override.or_else(|| {
            parsed
                .usage
                .as_ref()
                .map(|u| (u.prompt_tokens, u.completion_tokens))
        });
        let usage = resolve_usage(UsageResolution::from_output_text(
            reported,
            estimated_tokens,
            text,
        ));
        (usage, extras.billed_cost)
    }

    fn request_spans(&self, pair: &CandidatePair) -> Option<(Vec<CitedSpan>, Vec<CitedSpan>)> {
        let prompt = cross_link_user_prompt(pair);
        let target = ExplainTarget::CrossLink {
            from: pair.from,
            to: pair.to,
            kind: pair.kind,
        };

        let estimated_tokens = estimate_tokens(&prompt);
        if estimated_tokens > self.max_tokens_per_call {
            publish_budget_blocked(
                self.config.provider,
                &self.model,
                target,
                estimated_tokens,
                self.max_tokens_per_call,
            );
            return None;
        }

        let body = ChatRequest {
            model: &self.model,
            max_tokens: COMMENTARY_MAX_OUTPUT_TOKENS,
            messages: vec![
                ChatMessage {
                    role: "system",
                    content: CROSS_LINK_SYSTEM_PROMPT,
                },
                ChatMessage {
                    role: "user",
                    content: &prompt,
                },
            ],
        };

        let auth_header = format!("Bearer {}", self.api_key);
        let headers = self.build_headers(&auth_header);

        let ctx = CallCtx::start(self.config.provider, &self.model, target);
        let parsed: ChatResponse =
            match post_json_strict(&self.client, self.config.api_url, &headers, &body) {
                Ok(p) => p,
                Err(e) => {
                    ctx.fail(e);
                    return None;
                }
            };

        let text = parsed
            .choices
            .first()
            .and_then(|c| c.message.content.as_ref())
            .map(|s| s.trim().to_string())
            .unwrap_or_default();

        let extras = self.resolve_extras(&parsed, &headers);
        let (usage, cost) = self.resolve_response_usage(&parsed, &extras, estimated_tokens, &text);
        ctx.complete_with_cost(usage, cost, cap_output_bytes(&text));

        parse_spans_from_text(&text, pair.from, pair.to)
    }
}

impl CommentaryGenerator for OpenAiCompatProvider {
    fn generate(&self, node: NodeId, context: &str) -> crate::Result<Option<CommentaryEntry>> {
        let target = ExplainTarget::Commentary { node };

        let estimated_tokens = estimate_tokens(context);
        if estimated_tokens > self.max_tokens_per_call {
            publish_budget_blocked(
                self.config.provider,
                &self.model,
                target,
                estimated_tokens,
                self.max_tokens_per_call,
            );
            return Ok(None);
        }

        let body = ChatRequest {
            model: &self.model,
            max_tokens: COMMENTARY_MAX_OUTPUT_TOKENS,
            messages: vec![
                ChatMessage {
                    role: "system",
                    content: COMMENTARY_SYSTEM_PROMPT,
                },
                ChatMessage {
                    role: "user",
                    content: context,
                },
            ],
        };

        let auth_header = format!("Bearer {}", self.api_key);
        let headers = self.build_headers(&auth_header);

        let ctx = CallCtx::start(self.config.provider, &self.model, target);
        let parsed: ChatResponse =
            match post_json_strict(&self.client, self.config.api_url, &headers, &body) {
                Ok(p) => p,
                Err(e) => {
                    ctx.fail(e);
                    return Ok(None);
                }
            };

        // Resolve extras (OpenRouter generation stats) before consuming choices.
        let extras = self.resolve_extras(&parsed, &headers);
        let reported_raw = parsed
            .usage
            .as_ref()
            .map(|u| (u.prompt_tokens, u.completion_tokens));

        let raw_text = parsed
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
            .unwrap_or_default();
        let Some(text) = sanitize_generated_commentary_text(&raw_text) else {
            let reported = extras.usage_override.or(reported_raw);
            let usage = resolve_usage(UsageResolution::from_output_text(
                reported,
                estimated_tokens,
                "",
            ));
            ctx.complete_with_cost(usage, extras.billed_cost, 0);
            return Ok(None);
        };

        let reported = extras.usage_override.or(reported_raw);
        let usage = resolve_usage(UsageResolution::from_output_text(
            reported,
            estimated_tokens,
            &text,
        ));
        ctx.complete_with_cost(usage, extras.billed_cost, cap_output_bytes(&text));

        if text.is_empty() {
            return Ok(None);
        }

        Ok(Some(CommentaryEntry {
            node_id: node,
            text,
            provenance: crate::overlay::CommentaryProvenance {
                source_content_hash: String::new(),
                pass_id: self.config.pass_id.to_string(),
                model_identity: self.model.clone(),
                generated_at: OffsetDateTime::now_utc(),
            },
        }))
    }
}

impl CrossLinkGenerator for OpenAiCompatProvider {
    fn generate_candidates(
        &self,
        scope: &crate::pipeline::explain::cross_link::CandidateScope,
    ) -> crate::Result<Vec<OverlayLink>> {
        let thresholds = self
            .thresholds
            .expect("cross-link generator must have thresholds");
        Ok(build_overlay_links(
            scope,
            thresholds,
            self.config.cross_link_pass_id,
            &self.model,
            |pair| self.request_spans(pair),
        ))
    }
}

use crate::overlay::OverlayLink;

// Shared request/response types for OpenAI-compatible APIs.

/// OpenAI chat completion request.
#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    messages: Vec<ChatMessage<'a>>,
}

/// A single message in a chat completion request.
#[derive(Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

/// OpenAI chat completion response.
#[derive(Deserialize)]
pub struct ChatResponse {
    /// Response ID (used by OpenRouter for generation stats).
    #[serde(default)]
    pub id: Option<String>,
    /// Completion choices.
    pub choices: Vec<Choice>,
    /// Token usage, if reported.
    #[serde(default)]
    pub usage: Option<ChatUsage>,
}

/// Token usage reported by the API.
#[derive(Deserialize)]
pub struct ChatUsage {
    /// Input/prompt tokens.
    #[serde(default)]
    pub prompt_tokens: u32,
    /// Output/completion tokens.
    #[serde(default)]
    pub completion_tokens: u32,
}

/// A single completion choice.
#[derive(Deserialize)]
pub struct Choice {
    /// The generated message.
    pub message: MessageContent,
}

/// Message content within a choice.
#[derive(Deserialize)]
pub struct MessageContent {
    /// Generated text content.
    #[serde(default)]
    pub content: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ids::SymbolNodeId;
    use crate::overlay::ConfidenceThresholds;

    const TEST_CONFIG: OpenAiCompatConfig = OpenAiCompatConfig {
        provider: "test",
        api_url: "https://example.com/v1/chat/completions",
        pass_id: "commentary-v2-test",
        cross_link_pass_id: "cross-link-v1-test",
        default_model: "test-model",
        extra_headers: &[],
        on_response: None,
    };

    #[test]
    fn new_commentary_constructs_without_panicking() {
        let gen = OpenAiCompatProvider::new_commentary(
            &TEST_CONFIG,
            "fake-key".to_string(),
            "test-model".to_string(),
            5000,
        );
        let node = NodeId::Symbol(SymbolNodeId(1));
        let _ = gen.generate(node, "context");
    }

    #[test]
    fn oversized_context_skips_generation() {
        let context = "x".repeat(50_000);
        let gen = OpenAiCompatProvider::new_commentary(
            &TEST_CONFIG,
            "fake-key".to_string(),
            "test-model".to_string(),
            5000,
        );
        let node = NodeId::Symbol(SymbolNodeId(1));
        let entry = gen.generate(node, &context).unwrap();
        assert!(entry.is_none(), "oversized context must skip generation");
    }

    #[test]
    fn new_cross_link_constructs_without_panicking() {
        let _gen = OpenAiCompatProvider::new_cross_link(
            &TEST_CONFIG,
            "fake-key".to_string(),
            "test-model".to_string(),
            5000,
            ConfidenceThresholds::default(),
        );
    }
}
