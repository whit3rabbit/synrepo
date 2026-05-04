//! Z.ai (Zhipu / GLM) explain provider.
//!
//! Calls the Z.ai OpenAI-compatible Chat Completions API using a blocking
//! `reqwest::Client`. API key is read from `ZAI_API_KEY`. Model can be
//! overridden via `SYNREPO_LLM_MODEL`. The request/response shape mirrors
//! `openai.rs` because Z.ai exposes an OpenAI-compatible endpoint at
//! `/api/paas/v4/chat/completions`.

use crate::overlay::ConfidenceThresholds;
use crate::pipeline::explain::{CommentaryGenerator, CrossLinkGenerator};

use super::openai_compat::{OpenAiCompatConfig, OpenAiCompatProvider};

/// Default Z.ai model for explain (GLM-4.6 is the current flagship, same
/// list price as GLM-4.5 but stronger agentic coding).
pub const DEFAULT_MODEL: &str = "glm-4.6";

const CONFIG: OpenAiCompatConfig = OpenAiCompatConfig {
    provider: "zai",
    api_url: "https://api.z.ai/api/paas/v4/chat/completions",
    pass_id: "commentary-v4-zai",
    cross_link_pass_id: "cross-link-v1-zai",
    default_model: DEFAULT_MODEL,
    extra_headers: &[],
    on_response: None,
    on_response_async: None,
};

/// Z.ai-backed commentary generator.
pub type ZaiCommentaryGenerator = OpenAiCompatProvider;

/// Z.ai-backed cross-link generator.
pub type ZaiCrossLinkGenerator = OpenAiCompatProvider;

/// Create a new Z.ai commentary generator.
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

/// Create a new Z.ai cross-link generator.
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
