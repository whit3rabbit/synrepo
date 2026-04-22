//! Local synthesis provider (Ollama/vLLM).
//!
//! Calls a local LLM server. Default endpoint is `http://localhost:11434/api/chat` (Ollama).
//! Can be overridden via `SYNREPO_LLM_LOCAL_ENDPOINT`. If the endpoint path ends with
//! `/v1/chat/completions`, assumes OpenAI-compatible API; otherwise assumes Ollama native format.

use time::OffsetDateTime;

use crate::core::ids::NodeId;
use crate::overlay::{CitedSpan, ConfidenceThresholds, OverlayLink};
use crate::pipeline::synthesis::cross_link::{CandidatePair, CandidateScope};
use crate::pipeline::synthesis::telemetry::{publish_budget_blocked, CallCtx, SynthesisTarget};
use crate::pipeline::synthesis::{CommentaryEntry, CommentaryGenerator, CrossLinkGenerator};

use super::http::{
    build_client, cap_output_bytes, estimate_tokens, post_json_strict, resolve_usage,
    UsageResolution,
};
use super::shared::*;

const PROVIDER: &str = "local";

/// Default local model for synthesis.
pub const DEFAULT_MODEL: &str = "llama3";

/// Default Ollama endpoint.
const DEFAULT_ENDPOINT: &str = "http://localhost:11434/api/chat";

/// Pass IDs for local provider.
const PASS_ID: &str = "commentary-v1-local";
const CROSS_LINK_PASS_ID: &str = "cross-link-v1-local";

/// Local-backed commentary generator.
pub struct LocalCommentaryGenerator {
    endpoint: String,
    model: String,
    max_tokens_per_call: u32,
    client: reqwest::blocking::Client,
    is_openai_compatible: bool,
}

impl LocalCommentaryGenerator {
    /// Construct a generator with default endpoint.
    pub fn new(model: String, max_tokens_per_call: u32) -> Self {
        Self::with_endpoint(model, max_tokens_per_call, DEFAULT_ENDPOINT)
    }

    /// Construct a generator with a custom endpoint.
    pub fn with_endpoint(model: String, max_tokens_per_call: u32, endpoint: &str) -> Self {
        let is_openai_compatible = endpoint.ends_with("/v1/chat/completions");
        let client = build_client();
        Self {
            endpoint: endpoint.to_string(),
            model,
            max_tokens_per_call,
            client,
            is_openai_compatible,
        }
    }
}

impl CommentaryGenerator for LocalCommentaryGenerator {
    fn generate(&self, node: NodeId, context: &str) -> crate::Result<Option<CommentaryEntry>> {
        let target = SynthesisTarget::Commentary { node };

        let estimated_tokens = estimate_tokens(context);
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

        let body = if self.is_openai_compatible {
            serde_json::json!({
                "model": self.model,
                "max_tokens": 512,
                "messages": [
                    { "role": "system", "content": COMMENTARY_SYSTEM_PROMPT },
                    { "role": "user", "content": context }
                ]
            })
        } else {
            serde_json::json!({
                "model": self.model,
                "stream": false,
                "options": { "num_predict": 512 },
                "prompt": format!("System: {COMMENTARY_SYSTEM_PROMPT}\n\nUser: {context}")
            })
        };

        let headers = [("Content-Type", "application/json")];

        let ctx = CallCtx::start(PROVIDER, &self.model, target);
        let (text, usage) = if self.is_openai_compatible {
            match post_json_strict::<serde_json::Value, OpenAiCompatResponse>(
                &self.client,
                &self.endpoint,
                &headers,
                &body,
            ) {
                Ok(resp) => {
                    let usage = resolve_usage(UsageResolution::from_output_text(
                        resp.usage
                            .as_ref()
                            .filter(|u| u.prompt_tokens > 0 || u.completion_tokens > 0)
                            .map(|u| (u.prompt_tokens, u.completion_tokens)),
                        estimated_tokens,
                        resp.choices
                            .first()
                            .map(|c| c.message.content.as_str())
                            .unwrap_or_default(),
                    ));
                    let text = resp
                        .choices
                        .into_iter()
                        .next()
                        .map(|c| c.message.content)
                        .unwrap_or_default();
                    (text, usage)
                }
                Err(e) => {
                    ctx.fail(e);
                    return Ok(None);
                }
            }
        } else {
            match post_json_strict::<serde_json::Value, OllamaResponse>(
                &self.client,
                &self.endpoint,
                &headers,
                &body,
            ) {
                Ok(resp) => {
                    let usage = resolve_usage(UsageResolution::from_output_text(
                        resp.prompt_eval_count.zip(resp.eval_count),
                        estimated_tokens,
                        &resp.response,
                    ));
                    (resp.response, usage)
                }
                Err(e) => {
                    ctx.fail(e);
                    return Ok(None);
                }
            }
        };

        let text = text.trim().to_string();
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

/// Local-backed cross-link generator.
pub struct LocalCrossLinkGenerator {
    endpoint: String,
    model: String,
    max_tokens_per_call: u32,
    thresholds: ConfidenceThresholds,
    client: reqwest::blocking::Client,
    is_openai_compatible: bool,
}

impl LocalCrossLinkGenerator {
    /// Construct a generator with default endpoint.
    pub fn new(model: String, max_tokens_per_call: u32, thresholds: ConfidenceThresholds) -> Self {
        Self::with_endpoint(model, max_tokens_per_call, thresholds, DEFAULT_ENDPOINT)
    }

    /// Construct a generator with a custom endpoint.
    pub fn with_endpoint(
        model: String,
        max_tokens_per_call: u32,
        thresholds: ConfidenceThresholds,
        endpoint: &str,
    ) -> Self {
        let is_openai_compatible = endpoint.ends_with("/v1/chat/completions");
        let client = build_client();
        Self {
            endpoint: endpoint.to_string(),
            model,
            max_tokens_per_call,
            thresholds,
            client,
            is_openai_compatible,
        }
    }

    fn request_spans(&self, pair: &CandidatePair) -> Option<(Vec<CitedSpan>, Vec<CitedSpan>)> {
        let prompt = cross_link_user_prompt(pair);

        let target = SynthesisTarget::CrossLink {
            from: pair.from,
            to: pair.to,
            kind: pair.kind,
        };

        let estimated_tokens = estimate_tokens(&prompt);
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

        let body = if self.is_openai_compatible {
            serde_json::json!({
                "model": self.model,
                "max_tokens": 512,
                "messages": [
                    { "role": "system", "content": CROSS_LINK_SYSTEM_PROMPT },
                    { "role": "user", "content": prompt }
                ]
            })
        } else {
            serde_json::json!({
                "model": self.model,
                "stream": false,
                "options": { "num_predict": 512 },
                "prompt": format!("System: {CROSS_LINK_SYSTEM_PROMPT}\n\nUser: {prompt}")
            })
        };

        let headers = [("Content-Type", "application/json")];

        let ctx = CallCtx::start(PROVIDER, &self.model, target);
        let (text, usage) = if self.is_openai_compatible {
            match post_json_strict::<serde_json::Value, OpenAiCompatResponse>(
                &self.client,
                &self.endpoint,
                &headers,
                &body,
            ) {
                Ok(resp) => {
                    let usage = resolve_usage(UsageResolution::from_output_text(
                        resp.usage
                            .as_ref()
                            .filter(|u| u.prompt_tokens > 0 || u.completion_tokens > 0)
                            .map(|u| (u.prompt_tokens, u.completion_tokens)),
                        estimated_tokens,
                        resp.choices
                            .first()
                            .map(|c| c.message.content.as_str())
                            .unwrap_or_default(),
                    ));
                    let text = resp
                        .choices
                        .into_iter()
                        .next()
                        .map(|c| c.message.content)
                        .unwrap_or_default();
                    (text, usage)
                }
                Err(e) => {
                    ctx.fail(e);
                    return None;
                }
            }
        } else {
            match post_json_strict::<serde_json::Value, OllamaResponse>(
                &self.client,
                &self.endpoint,
                &headers,
                &body,
            ) {
                Ok(resp) => {
                    let usage = resolve_usage(UsageResolution::from_output_text(
                        resp.prompt_eval_count.zip(resp.eval_count),
                        estimated_tokens,
                        &resp.response,
                    ));
                    (resp.response, usage)
                }
                Err(e) => {
                    ctx.fail(e);
                    return None;
                }
            }
        };

        ctx.complete(usage, cap_output_bytes(&text));

        parse_spans_from_text(&text, pair.from, pair.to)
    }
}

impl CrossLinkGenerator for LocalCrossLinkGenerator {
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

// Local-provider response types

use serde::Deserialize;

#[derive(Deserialize)]
struct OpenAiCompatResponse {
    choices: Vec<OpenAiCompatChoice>,
    #[serde(default)]
    usage: Option<OpenAiCompatUsage>,
}

#[derive(Deserialize)]
struct OpenAiCompatChoice {
    message: OpenAiCompatMessage,
}

#[derive(Deserialize)]
struct OpenAiCompatMessage {
    #[serde(default)]
    content: String,
}

#[derive(Deserialize)]
struct OpenAiCompatUsage {
    #[serde(default)]
    prompt_tokens: u32,
    #[serde(default)]
    completion_tokens: u32,
}

#[derive(Deserialize)]
struct OllamaResponse {
    response: String,
    #[serde(default)]
    prompt_eval_count: Option<u32>,
    #[serde(default)]
    eval_count: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ids::SymbolNodeId;

    #[test]
    fn new_constructs_without_panicking() {
        let gen = LocalCommentaryGenerator::new("test-model".to_string(), 5000);
        let node = NodeId::Symbol(SymbolNodeId(1));
        // This will fail (no local server) but shouldn't panic
        let _ = gen.generate(node, "context");
    }

    #[test]
    fn oversized_context_skips_generation() {
        let context = "x".repeat(50_000);
        let gen = LocalCommentaryGenerator::new("test-model".to_string(), 5000);
        let node = NodeId::Symbol(SymbolNodeId(1));
        let entry = gen.generate(node, &context).unwrap();
        assert!(entry.is_none(), "oversized context must skip generation");
    }

    #[test]
    fn endpoint_parsing() {
        // Ollama default
        let gen = LocalCommentaryGenerator::new("llama3".to_string(), 5000);
        assert!(!gen.is_openai_compatible);

        // vLLM OpenAI-compatible
        let gen = LocalCommentaryGenerator::with_endpoint(
            "llama3".to_string(),
            5000,
            "http://localhost:8000/v1/chat/completions",
        );
        assert!(gen.is_openai_compatible);
    }
}
