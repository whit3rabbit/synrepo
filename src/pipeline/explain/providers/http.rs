//! Shared HTTP primitives for explain providers.
//!
//! Provides common utilities for building blocking HTTP clients, estimating
//! token counts, and making JSON requests.

use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::pipeline::explain::telemetry::TokenUsage;

/// Default timeout for explain API calls (30 seconds).
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Conservative chars-per-token ratio for budget estimation.
pub const CHARS_PER_TOKEN: u32 = 4;

/// Build a blocking HTTP client with the default timeout.
/// Falls back to a default client if builder configuration fails.
pub fn build_client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .timeout(DEFAULT_TIMEOUT)
        .build()
        .unwrap_or_else(|_| reqwest::blocking::Client::new())
}

/// Estimate the number of tokens in a context string using the chars-per-token ratio.
pub fn estimate_tokens(context: &str) -> u32 {
    (context.len() as u32) / CHARS_PER_TOKEN
}

/// Estimate completion tokens from the generated output text.
pub fn estimate_output_tokens(output_text: &str) -> u32 {
    estimate_tokens(output_text)
}

/// Inputs used to resolve the final token accounting for one provider call.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UsageResolution {
    /// Prompt/input tokens estimated before or during the call.
    pub estimated_input_tokens: u32,
    /// Completion/output tokens estimated from the accepted output text.
    pub estimated_output_tokens: u32,
    /// Provider-reported usage, when available.
    pub reported_usage: Option<(u32, u32)>,
}

impl UsageResolution {
    /// Construct a usage resolution from the accepted output text.
    pub fn from_output_text(
        reported_usage: Option<(u32, u32)>,
        estimated_input_tokens: u32,
        output_text: &str,
    ) -> Self {
        Self {
            estimated_input_tokens,
            estimated_output_tokens: estimate_output_tokens(output_text),
            reported_usage,
        }
    }
}

/// Turn a [`UsageResolution`] into a final [`TokenUsage`].
pub fn resolve_usage(resolution: UsageResolution) -> TokenUsage {
    match resolution.reported_usage {
        Some((input_tokens, output_tokens)) => TokenUsage::reported(input_tokens, output_tokens),
        None => TokenUsage::estimated(
            resolution.estimated_input_tokens,
            resolution.estimated_output_tokens,
        ),
    }
}

/// Clamp a text length to a `u32`, used when publishing `output_bytes` on a
/// completion event. Avoids five copies of the `text.len().min(u32::MAX as usize) as u32`
/// pattern across providers.
pub fn cap_output_bytes(text: &str) -> u32 {
    text.len().min(u32::MAX as usize) as u32
}

/// JSON POST variant that returns a descriptive failure string instead of
/// collapsing all failure modes to `Ok(None)`. Used by telemetry-instrumented
/// providers so `CallCtx::fail` gets a meaningful error tail.
///
/// The returned string is short and safe to log: transport errors include
/// the reqwest message, non-success status includes the numeric status only
/// (no response body), and parse errors include the serde message.
pub fn post_json_strict<Req, Res>(
    client: &reqwest::blocking::Client,
    url: &str,
    headers: &[(&str, &str)],
    body: &Req,
) -> Result<Res, String>
where
    Req: Serialize,
    Res: DeserializeOwned,
{
    let mut request = client.post(url);
    for (key, value) in headers {
        request = request.header(*key, *value);
    }

    let response = request
        .json(body)
        .send()
        .map_err(|e| format!("transport error: {e}"))?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!("non-success status: {status}"));
    }

    response
        .json::<Res>()
        .map_err(|e| format!("response parse error: {e}"))
}

/// JSON GET variant that returns a descriptive failure string rather than
/// collapsing all failure modes to `Ok(None)`.
pub fn get_json_strict<Res>(
    client: &reqwest::blocking::Client,
    url: &str,
    headers: &[(&str, &str)],
) -> Result<Res, String>
where
    Res: DeserializeOwned,
{
    let mut request = client.get(url);
    for (key, value) in headers {
        request = request.header(*key, *value);
    }

    let response = request
        .send()
        .map_err(|e| format!("transport error: {e}"))?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!("non-success status: {status}"));
    }

    response
        .json::<Res>()
        .map_err(|e| format!("response parse error: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::explain::telemetry::UsageSource;

    #[test]
    fn estimated_completion_tokens_follow_output_text() {
        let usage = resolve_usage(UsageResolution::from_output_text(None, 400, "tiny"));
        assert_eq!(usage.input_tokens, 400);
        assert_eq!(usage.output_tokens, estimate_output_tokens("tiny"));
        assert_eq!(usage.source, UsageSource::Estimated);
    }

    #[test]
    fn reported_usage_wins_over_estimates() {
        let usage = resolve_usage(UsageResolution::from_output_text(
            Some((11, 7)),
            400,
            "tiny",
        ));
        assert_eq!(usage.input_tokens, 11);
        assert_eq!(usage.output_tokens, 7);
        assert_eq!(usage.source, UsageSource::Reported);
    }
}
