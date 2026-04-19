//! Shared HTTP primitives for synthesis providers.
//!
//! Provides common utilities for building blocking HTTP clients, estimating
//! token counts, and making JSON requests.

use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::Serialize;

/// Default timeout for synthesis API calls (30 seconds).
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

/// Make a JSON POST request and parse the response.
/// Returns `Ok(None)` on any failure (network error, non-success status, JSON parse error).
/// The caller is responsible for adding provider-specific headers.
pub fn post_json<Req, Res>(
    client: &reqwest::blocking::Client,
    url: &str,
    headers: &[(&str, &str)],
    body: &Req,
) -> crate::Result<Option<Res>>
where
    Req: Serialize,
    Res: DeserializeOwned,
{
    let mut request = client.post(url);
    for (key, value) in headers {
        request = request.header(*key, *value);
    }

    let response = match request.json(body).send() {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "synthesis request failed");
            return Ok(None);
        }
    };

    if !response.status().is_success() {
        tracing::warn!(status = %response.status(), "synthesis returned non-success");
        return Ok(None);
    }

    match response.json() {
        Ok(parsed) => Ok(Some(parsed)),
        Err(e) => {
            tracing::warn!(error = %e, "synthesis response parse failed");
            Ok(None)
        }
    }
}
