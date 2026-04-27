//! OpenRouter explain provider.
//!
//! Calls the OpenRouter API (OpenAI-compatible) using a blocking `reqwest::Client`.
//! API key is read from `OPENROUTER_API_KEY`. Model can be overridden via `SYNREPO_LLM_MODEL`.
//!
//! Extends the generic [`OpenAiCompatProvider`] with a post-response hook that fetches
//! generation statistics for accurate usage and cost tracking.

use serde::Deserialize;

use crate::overlay::ConfidenceThresholds;
use crate::pipeline::explain::{CommentaryGenerator, CrossLinkGenerator};

use super::http::get_json_strict;
use super::openai_compat::{
    ChatResponse, OpenAiCompatConfig, OpenAiCompatProvider, ResponseExtras,
};

/// Default OpenRouter model for explain (Gemma is usually free/cheap).
pub const DEFAULT_MODEL: &str = "google/gemma-4-31b-it:free";

const GENERATION_URL: &str = "https://openrouter.ai/api/v1/generation";

const CONFIG: OpenAiCompatConfig = OpenAiCompatConfig {
    provider: "openrouter",
    api_url: "https://openrouter.ai/api/v1/chat/completions",
    pass_id: "commentary-v4-openrouter",
    cross_link_pass_id: "cross-link-v1-openrouter",
    default_model: DEFAULT_MODEL,
    extra_headers: &[
        ("HTTP-Referer", "https://github.com/whit3rabbit/synrepo"),
        ("X-Title", "synrepo"),
    ],
    on_response: Some(openrouter_response_hook),
};

/// OpenRouter-backed commentary generator.
pub type OpenRouterCommentaryGenerator = OpenAiCompatProvider;

/// OpenRouter-backed cross-link generator.
pub type OpenRouterCrossLinkGenerator = OpenAiCompatProvider;

/// Create a new OpenRouter commentary generator.
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

/// Create a new OpenRouter cross-link generator.
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

/// Generation statistics returned by the OpenRouter generation API.
#[derive(Deserialize)]
struct GenerationStats {
    #[serde(default)]
    total_cost: Option<f64>,
    #[serde(default)]
    cost: Option<f64>,
    #[serde(default)]
    native_tokens_prompt: Option<u32>,
    #[serde(default)]
    native_tokens_completion: Option<u32>,
    #[serde(default)]
    tokens_prompt: Option<u32>,
    #[serde(default)]
    tokens_completion: Option<u32>,
}

impl GenerationStats {
    fn usage_pair(&self) -> Option<(u32, u32)> {
        self.native_tokens_prompt
            .zip(self.native_tokens_completion)
            .or_else(|| self.tokens_prompt.zip(self.tokens_completion))
    }

    fn billed_cost(&self) -> Option<f64> {
        self.total_cost.or(self.cost)
    }
}

fn fetch_generation_stats(
    client: &reqwest::blocking::Client,
    headers: &[(&str, &str)],
    generation_id: &str,
) -> Option<GenerationStats> {
    let url = format!("{GENERATION_URL}?id={generation_id}");
    get_json_strict(client, &url, headers).ok()
}

/// Post-response hook: fetch generation stats for usage/cost overrides.
fn openrouter_response_hook(
    parsed: &ChatResponse,
    client: &reqwest::blocking::Client,
    headers: &[(&str, &str)],
) -> ResponseExtras {
    let stats = parsed
        .id
        .as_deref()
        .and_then(|id| fetch_generation_stats(client, headers, id));

    let usage_override = stats.as_ref().and_then(GenerationStats::usage_pair);
    let billed_cost = stats.as_ref().and_then(GenerationStats::billed_cost);

    // If the response reported non-zero usage, prefer that over generation stats.
    let usage_override = parsed
        .usage
        .as_ref()
        .filter(|u| u.prompt_tokens > 0 || u.completion_tokens > 0)
        .map(|u| (u.prompt_tokens, u.completion_tokens))
        .or(usage_override);

    ResponseExtras {
        usage_override,
        billed_cost,
    }
}
