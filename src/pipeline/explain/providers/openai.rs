//! OpenAI explain provider.
//!
//! Calls the OpenAI Chat Completions API using a blocking `reqwest::Client`. API key is read from
//! `OPENAI_API_KEY`. Model can be overridden via `SYNREPO_LLM_MODEL`.

use crate::overlay::ConfidenceThresholds;
use crate::pipeline::explain::{CommentaryGenerator, CrossLinkGenerator};

use super::openai_compat::{OpenAiCompatConfig, OpenAiCompatProvider};

/// Default OpenAI model for explain (gpt-4o-mini is cheap and reliable).
pub const DEFAULT_MODEL: &str = "gpt-4o-mini";

const CONFIG: OpenAiCompatConfig = OpenAiCompatConfig {
    provider: "openai",
    api_url: "https://api.openai.com/v1/chat/completions",
    pass_id: "commentary-v4-openai",
    cross_link_pass_id: "cross-link-v1-openai",
    default_model: DEFAULT_MODEL,
    extra_headers: &[],
    on_response: None,
    on_response_async: None,
};

/// OpenAI-backed commentary generator.
pub type OpenAiCommentaryGenerator = OpenAiCompatProvider;

/// OpenAI-backed cross-link generator.
pub type OpenAiCrossLinkGenerator = OpenAiCompatProvider;

/// Create a new OpenAI commentary generator.
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

/// Create a new OpenAI cross-link generator.
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
