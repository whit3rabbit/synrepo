//! Anthropic (Claude) explain provider.
//!
//! Calls the Claude Messages API using a blocking `reqwest::Client`. API key is read from
//! `ANTHROPIC_API_KEY` (or deprecated `SYNREPO_ANTHROPIC_API_KEY`). If no key is available,
//! the factory returns a `NoOpGenerator`.

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

const PROVIDER: &str = "anthropic";

/// Default Anthropic model for explain.
pub const DEFAULT_MODEL: &str = "claude-sonnet-4-6";

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const COUNT_TOKENS_URL: &str = "https://api.anthropic.com/v1/messages/count_tokens";
const API_VERSION: &str = "2023-06-01";
const PASS_ID: &str = "commentary-v2";
const CROSS_LINK_PASS_ID: &str = "cross-link-v1";

/// Anthropic-backed commentary generator.
pub struct AnthropicCommentaryGenerator {
    api_key: String,
    model: String,
    max_tokens_per_call: u32,
    client: reqwest::blocking::Client,
}

impl AnthropicCommentaryGenerator {
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

impl CommentaryGenerator for AnthropicCommentaryGenerator {
    fn generate(&self, node: NodeId, context: &str) -> crate::Result<Option<CommentaryEntry>> {
        let target = ExplainTarget::Commentary { node };
        let count_request = TokenCountRequest {
            model: &self.model,
            system: COMMENTARY_SYSTEM_PROMPT,
            messages: vec![Message {
                role: "user",
                content: context,
            }],
        };

        let estimated_tokens = count_input_tokens(&self.client, &self.api_key, &count_request)
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

        let body = MessagesRequest {
            model: &self.model,
            max_tokens: 512,
            system: COMMENTARY_SYSTEM_PROMPT,
            messages: vec![Message {
                role: "user",
                content: context,
            }],
        };

        let headers = [
            ("x-api-key", self.api_key.as_str()),
            ("anthropic-version", API_VERSION),
            ("content-type", "application/json"),
        ];

        let ctx = CallCtx::start(PROVIDER, &self.model, target);
        let parsed: MessagesResponse =
            match post_json_strict(&self.client, API_URL, &headers, &body) {
                Ok(p) => p,
                Err(e) => {
                    ctx.fail(e);
                    return Ok(None);
                }
            };

        let text = sanitize_commentary_text(
            &parsed
                .content
                .into_iter()
                .filter(|block| block.ty == "text")
                .map(|block| block.text)
                .collect::<Vec<_>>()
                .join("\n"),
        );

        let usage = resolve_usage(UsageResolution::from_output_text(
            parsed
                .usage
                .as_ref()
                .map(|u| (u.input_tokens, u.output_tokens)),
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

/// Anthropic-backed cross-link generator.
pub struct AnthropicCrossLinkGenerator {
    api_key: String,
    model: String,
    max_tokens_per_call: u32,
    thresholds: ConfidenceThresholds,
    client: reqwest::blocking::Client,
}

impl AnthropicCrossLinkGenerator {
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
        let count_request = TokenCountRequest {
            model: &self.model,
            system: CROSS_LINK_SYSTEM_PROMPT,
            messages: vec![Message {
                role: "user",
                content: &prompt,
            }],
        };

        let estimated_tokens = count_input_tokens(&self.client, &self.api_key, &count_request)
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

        let body = MessagesRequest {
            model: &self.model,
            max_tokens: 512,
            system: CROSS_LINK_SYSTEM_PROMPT,
            messages: vec![Message {
                role: "user",
                content: &prompt,
            }],
        };

        let headers = [
            ("x-api-key", self.api_key.as_str()),
            ("anthropic-version", API_VERSION),
            ("content-type", "application/json"),
        ];

        let ctx = CallCtx::start(PROVIDER, &self.model, target);
        let parsed: MessagesResponse =
            match post_json_strict(&self.client, API_URL, &headers, &body) {
                Ok(p) => p,
                Err(e) => {
                    ctx.fail(e);
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

        let usage = resolve_usage(UsageResolution::from_output_text(
            parsed
                .usage
                .as_ref()
                .map(|u| (u.input_tokens, u.output_tokens)),
            estimated_tokens,
            &text,
        ));
        ctx.complete(usage, cap_output_bytes(&text));

        parse_spans_from_text(&text, pair.from, pair.to)
    }
}

impl CrossLinkGenerator for AnthropicCrossLinkGenerator {
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

// Anthropic-specific request/response types

use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct MessagesRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    system: &'a str,
    messages: Vec<Message<'a>>,
}

#[derive(Serialize)]
struct TokenCountRequest<'a> {
    model: &'a str,
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
    #[serde(default)]
    usage: Option<AnthropicUsage>,
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    ty: String,
    #[serde(default)]
    text: String,
}

#[derive(Deserialize)]
struct AnthropicUsage {
    #[serde(default)]
    input_tokens: u32,
    #[serde(default)]
    output_tokens: u32,
}

#[derive(Deserialize)]
struct TokenCountResponse {
    #[serde(default)]
    input_tokens: u32,
}

fn count_input_tokens(
    client: &reqwest::blocking::Client,
    api_key: &str,
    request: &TokenCountRequest<'_>,
) -> Option<u32> {
    let headers = [
        ("x-api-key", api_key),
        ("anthropic-version", API_VERSION),
        ("content-type", "application/json"),
    ];
    post_json_strict::<TokenCountRequest<'_>, TokenCountResponse>(
        client,
        COUNT_TOKENS_URL,
        &headers,
        request,
    )
    .ok()
    .map(|response| response.input_tokens)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ids::SymbolNodeId;

    #[test]
    fn new_constructs_without_panicking() {
        let gen = AnthropicCommentaryGenerator::new(
            "fake-key".to_string(),
            "test-model".to_string(),
            5000,
        );
        let node = NodeId::Symbol(SymbolNodeId(1));
        // This will fail (no API key) but shouldn't panic
        let _ = gen.generate(node, "context");
    }

    #[test]
    fn oversized_context_skips_generation() {
        let context = "x".repeat(50_000);
        let gen = AnthropicCommentaryGenerator::new(
            "fake-key".to_string(),
            "test-model".to_string(),
            5000,
        );
        let node = NodeId::Symbol(SymbolNodeId(1));
        let entry = gen.generate(node, &context).unwrap();
        assert!(entry.is_none(), "oversized context must skip generation");
    }
}
