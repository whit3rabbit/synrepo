use std::sync::OnceLock;

use serde::de::DeserializeOwned;
use serde::Serialize;

use super::{parse_retry_after, sanitize_reqwest_error, HttpJsonError, DEFAULT_TIMEOUT};

/// Build an async HTTP client with the default timeout.
pub fn build_async_client() -> reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(build_async_pooled_client).clone()
}

fn build_async_pooled_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(DEFAULT_TIMEOUT)
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

/// Async JSON POST variant used by concurrent commentary refresh.
pub async fn post_json_strict_async<Req, Res>(
    client: &reqwest::Client,
    url: &str,
    headers: &[(&str, &str)],
    body: &Req,
) -> Result<Res, HttpJsonError>
where
    Req: Serialize + Sync,
    Res: DeserializeOwned,
{
    let mut request = client.post(url);
    for (key, value) in headers {
        request = request.header(*key, *value);
    }

    let response = request
        .json(body)
        .send()
        .await
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
        .await
        .map_err(|e| HttpJsonError::Parse(sanitize_reqwest_error(e)))
}

/// Async JSON GET variant used by async provider response hooks.
pub async fn get_json_strict_async<Res>(
    client: &reqwest::Client,
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
        .await
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
        .await
        .map_err(|e| HttpJsonError::Parse(sanitize_reqwest_error(e)))
}
