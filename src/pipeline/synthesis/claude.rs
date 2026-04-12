//! Claude-backed commentary generator.
//!
//! Calls the Claude Messages API using a blocking `reqwest::Client`. The API
//! key is read from the `SYNREPO_ANTHROPIC_API_KEY` environment variable. If
//! the key is missing, [`new_or_noop`] returns a [`super::NoOpGenerator`]
//! and no network call is ever attempted.
//!
//! This implementation is intentionally minimal: it is the first live
//! generation path and exists to validate the boundary. It does not retry,
//! does not stream, and does not parse tool use. Future slices can replace
//! it without changing the `CommentaryGenerator` trait.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::core::ids::NodeId;
use crate::overlay::{CommentaryEntry, CommentaryProvenance};

use super::{CommentaryGenerator, NoOpGenerator};

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2023-06-01";
const DEFAULT_MODEL: &str = "claude-sonnet-4-6";
const ENV_KEY: &str = "SYNREPO_ANTHROPIC_API_KEY";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const PASS_ID: &str = "commentary-overlay-v1";

/// Estimated chars-per-token ratio; conservative upper bound on how many
/// prompt tokens a context string will consume.
const CHARS_PER_TOKEN: u32 = 4;

/// A commentary generator that calls the Claude Messages API.
pub struct ClaudeCommentaryGenerator {
    api_key: String,
    model: String,
    max_tokens_per_call: u32,
    client: reqwest::blocking::Client,
}

impl ClaudeCommentaryGenerator {
    /// Construct a generator with an explicit API key. Prefer [`Self::new_or_noop`]
    /// for environment-based construction.
    pub fn new(api_key: String, max_tokens_per_call: u32) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new());
        Self {
            api_key,
            model: DEFAULT_MODEL.to_string(),
            max_tokens_per_call,
            client,
        }
    }

    /// Construct a live generator if `SYNREPO_ANTHROPIC_API_KEY` is set,
    /// otherwise return the [`NoOpGenerator`] as a boxed generator.
    ///
    /// This is the preferred constructor for callers that want to silently
    /// fall back when no key is configured.
    pub fn new_or_noop(max_tokens_per_call: u32) -> Box<dyn CommentaryGenerator> {
        match std::env::var(ENV_KEY) {
            Ok(key) if !key.is_empty() => Box::new(Self::new(key, max_tokens_per_call)),
            _ => Box::new(NoOpGenerator),
        }
    }
}

impl CommentaryGenerator for ClaudeCommentaryGenerator {
    fn generate(&self, node: NodeId, context: &str) -> crate::Result<Option<CommentaryEntry>> {
        // Cost check: estimated prompt tokens exceed the configured budget.
        let estimated_tokens = (context.len() as u32) / CHARS_PER_TOKEN;
        if estimated_tokens > self.max_tokens_per_call {
            tracing::warn!(
                estimated = estimated_tokens,
                budget = self.max_tokens_per_call,
                "commentary generation skipped: context exceeds configured cost limit"
            );
            return Ok(None);
        }

        let body = MessagesRequest {
            model: &self.model,
            max_tokens: 512,
            system: "Produce a single paragraph of at most three sentences explaining the \
                     intent and role of the given code symbol. Avoid restating the \
                     signature verbatim. If the context is ambiguous, return one \
                     sentence noting what is unclear.",
            messages: vec![Message {
                role: "user",
                content: context,
            }],
        };

        let response = self
            .client
            .post(API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send();

        let response = match response {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(error = %e, "commentary generation request failed");
                return Ok(None);
            }
        };

        if !response.status().is_success() {
            tracing::warn!(
                status = %response.status(),
                "commentary generation returned non-success status"
            );
            return Ok(None);
        }

        let parsed: MessagesResponse = match response.json() {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(error = %e, "commentary generation response parse failed");
                return Ok(None);
            }
        };

        let text = parsed
            .content
            .into_iter()
            .filter(|block| block.ty == "text")
            .map(|block| block.text)
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string();

        if text.is_empty() {
            return Ok(None);
        }

        Ok(Some(CommentaryEntry {
            node_id: node,
            text,
            provenance: CommentaryProvenance {
                // Filled in by the caller from the current graph state
                // before insert_commentary; we don't have access here.
                source_content_hash: String::new(),
                pass_id: PASS_ID.to_string(),
                model_identity: self.model.clone(),
                generated_at: OffsetDateTime::now_utc(),
            },
        }))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_or_noop_constructs_without_panicking() {
        // We don't touch env: if the test runner exports the key, we exercise
        // the live path's constructor; otherwise we exercise the NoOp path.
        // Either way, `generate` must not panic for a generic context.
        let generator = ClaudeCommentaryGenerator::new_or_noop(5000);
        let _ = generator.generate(NodeId::Symbol(crate::core::ids::SymbolNodeId(1)), "context");
    }

    #[test]
    fn oversized_context_skips_generation() {
        // A context well over 5000 tokens' worth of characters (4 chars/token ≈ 20k chars).
        let context = "x".repeat(50_000);
        let generator = ClaudeCommentaryGenerator::new("fake-key".to_string(), 5000);
        let entry = generator
            .generate(NodeId::Symbol(crate::core::ids::SymbolNodeId(1)), &context)
            .unwrap();
        assert!(entry.is_none(), "oversized context must skip generation");
    }
}
