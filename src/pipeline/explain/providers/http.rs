//! Shared HTTP primitives for explain providers.
//!
//! Provides common utilities for building HTTP clients, estimating
//! token counts, and making JSON requests.

use std::{sync::OnceLock, time::Duration};

use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::pipeline::explain::telemetry::{ExplainFailure, TokenUsage};

mod async_io;

pub use async_io::{build_async_client, get_json_strict_async, post_json_strict_async};

/// Default timeout for explain API calls (30 seconds).
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Conservative chars-per-token ratio for budget estimation.
pub const CHARS_PER_TOKEN: u32 = 3;

/// Build a blocking HTTP client with the default timeout.
/// Falls back to a default client if builder configuration fails.
pub fn build_client() -> reqwest::blocking::Client {
    static CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();
    CLIENT.get_or_init(build_pooled_client).clone()
}

fn build_pooled_client() -> reqwest::blocking::Client {
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

/// Typed JSON request failure used by explain providers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HttpJsonError {
    /// The request could not be sent or completed.
    Transport(String),
    /// The provider returned a non-success HTTP status.
    Status {
        /// HTTP status code.
        status: StatusCode,
        /// Parsed Retry-After delay, when the provider supplied seconds.
        retry_after: Option<Duration>,
    },
    /// Response JSON could not be parsed.
    Parse(String),
}

impl HttpJsonError {
    /// True when the provider explicitly rate-limited the request.
    pub fn is_rate_limited(&self) -> bool {
        matches!(
            self,
            Self::Status {
                status: StatusCode::TOO_MANY_REQUESTS,
                ..
            }
        )
    }

    /// Provider-advised retry delay, if present and parseable.
    pub fn retry_after(&self) -> Option<Duration> {
        match self {
            Self::Status { retry_after, .. } => *retry_after,
            _ => None,
        }
    }
}

impl std::fmt::Display for HttpJsonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transport(error) => {
                write!(f, "transport error: {}", redact_url_query_params(error))
            }
            Self::Status { status, .. } => write!(f, "non-success status: {status}"),
            Self::Parse(error) => write!(
                f,
                "response parse error: {}",
                redact_url_query_params(error)
            ),
        }
    }
}

impl std::error::Error for HttpJsonError {}

impl From<HttpJsonError> for ExplainFailure {
    fn from(error: HttpJsonError) -> Self {
        Self {
            error: error.to_string(),
            http_status: match &error {
                HttpJsonError::Status { status, .. } => Some(status.as_u16()),
                _ => None,
            },
            retry_after_ms: error
                .retry_after()
                .map(|duration| duration.as_millis().min(u64::MAX as u128) as u64),
        }
    }
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
) -> Result<Res, HttpJsonError>
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
        .map_err(|e| HttpJsonError::Transport(sanitize_reqwest_error(e)))?;

    let status = response.status();
    if !status.is_success() {
        return Err(HttpJsonError::Status {
            status,
            retry_after: parse_retry_after(response.headers()),
        });
    }

    response
        .json::<Res>()
        .map_err(|e| HttpJsonError::Parse(sanitize_reqwest_error(e)))
}

/// JSON GET variant that returns a descriptive failure string rather than
/// collapsing all failure modes to `Ok(None)`.
pub fn get_json_strict<Res>(
    client: &reqwest::blocking::Client,
    url: &str,
    headers: &[(&str, &str)],
) -> Result<Res, HttpJsonError>
where
    Res: DeserializeOwned,
{
    let mut request = client.get(url);
    for (key, value) in headers {
        request = request.header(*key, *value);
    }

    let response = request
        .send()
        .map_err(|e| HttpJsonError::Transport(sanitize_reqwest_error(e)))?;

    let status = response.status();
    if !status.is_success() {
        return Err(HttpJsonError::Status {
            status,
            retry_after: parse_retry_after(response.headers()),
        });
    }

    response
        .json::<Res>()
        .map_err(|e| HttpJsonError::Parse(sanitize_reqwest_error(e)))
}

fn sanitize_reqwest_error(error: reqwest::Error) -> String {
    redact_url_query_params(&error.without_url().to_string())
}

fn redact_url_query_params(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    while let Some(relative_query) = text[cursor..].find('?') {
        let query = cursor + relative_query;
        let token_start = text[..query]
            .rfind(char::is_whitespace)
            .map(|idx| idx + 1)
            .unwrap_or(0);
        if !text[token_start..query].contains("://") {
            out.push_str(&text[cursor..query + 1]);
            cursor = query + 1;
            continue;
        }

        out.push_str(&text[cursor..query]);
        let token_end = text[query..]
            .find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '`' | ')' | '<' | '>'))
            .map(|idx| query + idx)
            .unwrap_or(text.len());
        out.push_str("?[redacted-query]");
        cursor = token_end;
    }
    out.push_str(&text[cursor..]);
    out
}

fn parse_retry_after(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    let value = headers.get(reqwest::header::RETRY_AFTER)?;
    let seconds = value.to_str().ok()?.trim().parse::<u64>().ok()?;
    Some(Duration::from_secs(seconds))
}

#[cfg(test)]
mod tests;
