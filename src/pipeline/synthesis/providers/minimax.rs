//! MiniMax synthesis provider.
//!
//! Calls the MiniMax international OpenAI-compatible Chat Completions API
//! (`https://api.minimax.io/v1/chat/completions`) using a blocking
//! `reqwest::Client`. API key is read from `MINIMAX_API_KEY`. Model can be
//! overridden via `SYNREPO_LLM_MODEL`. The request/response shape mirrors
//! `openai.rs`; the China-mainland host (`api.minimaxi.chat`) exposes the
//! same shape and can be reached by setting `model` plus a custom endpoint
//! if you ever need it.

use crate::overlay::ConfidenceThresholds;
use crate::pipeline::synthesis::{CommentaryGenerator, CrossLinkGenerator};

use super::openai_compat::{OpenAiCompatConfig, OpenAiCompatProvider};

/// Default MiniMax model for synthesis (MiniMax-M2 is the current flagship
/// and is positioned as an inexpensive agentic-coding option).
pub const DEFAULT_MODEL: &str = "MiniMax-M2";

const CONFIG: OpenAiCompatConfig = OpenAiCompatConfig {
    provider: "minimax",
    api_url: "https://api.minimax.io/v1/chat/completions",
    pass_id: "commentary-v1-minimax",
    cross_link_pass_id: "cross-link-v1-minimax",
    default_model: DEFAULT_MODEL,
    extra_headers: &[],
    on_response: None,
};

/// MiniMax-backed commentary generator.
pub type MinimaxCommentaryGenerator = OpenAiCompatProvider;

/// MiniMax-backed cross-link generator.
pub type MinimaxCrossLinkGenerator = OpenAiCompatProvider;

/// Create a new MiniMax commentary generator.
pub fn new_commentary(
    api_key: String,
    model: String,
    max_tokens_per_call: u32,
) -> Box<dyn CommentaryGenerator> {
    Box::new(OpenAiCompatProvider::new_commentary(
        &CONFIG,
        api_key,
        model,
        max_tokens_per_call,
    ))
}

/// Create a new MiniMax cross-link generator.
pub fn new_cross_link(
    api_key: String,
    model: String,
    max_tokens_per_call: u32,
    thresholds: ConfidenceThresholds,
) -> Box<dyn CrossLinkGenerator> {
    Box::new(OpenAiCompatProvider::new_cross_link(
        &CONFIG,
        api_key,
        model,
        max_tokens_per_call,
        thresholds,
    ))
}
