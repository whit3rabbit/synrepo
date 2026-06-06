use super::*;
use crate::pipeline::explain::telemetry::UsageSource;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use time::macros::format_description;

#[test]
fn estimated_completion_tokens_follow_output_text() {
    let usage = resolve_usage(UsageResolution::from_output_text(None, 400, "tiny"));
    assert_eq!(usage.input_tokens, 400);
    assert_eq!(usage.output_tokens, estimate_output_tokens("tiny"));
    assert_eq!(usage.source, UsageSource::Estimated);
}

#[test]
fn estimate_tokens_uses_conservative_ratio() {
    assert_eq!(estimate_tokens("abcdef"), 2);
    assert_eq!(CHARS_PER_TOKEN, 3);
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

#[test]
fn status_error_formats_like_previous_string() {
    let error = HttpJsonError::Status {
        status: StatusCode::TOO_MANY_REQUESTS,
        retry_after: Some(Duration::from_secs(3)),
    };
    assert!(error.is_rate_limited());
    assert_eq!(error.retry_after(), Some(Duration::from_secs(3)));
    assert_eq!(
        error.to_string(),
        "non-success status: 429 Too Many Requests"
    );
}

#[test]
fn transport_and_parse_errors_keep_reason_prefixes() {
    assert_eq!(
        HttpJsonError::Transport("reset".to_string()).to_string(),
        "transport error: reset"
    );
    assert_eq!(
        HttpJsonError::Parse("bad json".to_string()).to_string(),
        "response parse error: bad json"
    );
    let leaked = "failed for url (https://example.test/v1?key=SECRET&alt=json)";
    let error = HttpJsonError::Transport(leaked.to_string()).to_string();
    assert!(error.contains("[redacted-query]"));
    assert!(!error.contains("SECRET"));
    assert!(!error.contains("key="));
}

#[test]
fn post_json_strict_captures_429_retry_after() {
    let url =
        serve_once("HTTP/1.1 429 Too Many Requests\r\nRetry-After: 2\r\nContent-Length: 0\r\n\r\n");
    let err = post_json_strict::<_, serde_json::Value>(
        &build_client(),
        &url,
        &[("Content-Type", "application/json")],
        &serde_json::json!({"x": 1}),
    )
    .unwrap_err();

    assert!(err.is_rate_limited());
    assert_eq!(err.retry_after(), Some(Duration::from_secs(2)));
    assert_eq!(err.to_string(), "non-success status: 429 Too Many Requests");
}

#[test]
fn retry_after_parses_http_date_in_the_future() {
    let future = OffsetDateTime::now_utc() + time::Duration::seconds(10);
    let delay = retry_after_from_header(&format_http_date(future)).unwrap();

    assert!(delay > Duration::ZERO);
    assert!(delay <= Duration::from_secs(10));
}

#[test]
fn retry_after_ignores_malformed_values() {
    assert_eq!(retry_after_from_header("not a retry-after"), None);
}

#[test]
fn retry_after_ignores_past_http_dates() {
    let past = OffsetDateTime::now_utc() - time::Duration::seconds(10);

    assert_eq!(retry_after_from_header(&format_http_date(past)), None);
}

#[test]
fn post_json_strict_reports_status_and_parse_failures() {
    let status_url = serve_once("HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n");
    let status_err = post_json_strict::<_, serde_json::Value>(
        &build_client(),
        &status_url,
        &[("Content-Type", "application/json")],
        &serde_json::json!({"x": 1}),
    )
    .unwrap_err();
    assert!(matches!(
        status_err,
        HttpJsonError::Status {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            ..
        }
    ));

    let parse_url = serve_once(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 8\r\n\r\nnot-json",
    );
    let parse_err = post_json_strict::<_, serde_json::Value>(
        &build_client(),
        &parse_url,
        &[("Content-Type", "application/json")],
        &serde_json::json!({"x": 1}),
    )
    .unwrap_err();
    assert!(matches!(parse_err, HttpJsonError::Parse(_)));
}

fn retry_after_from_header(value: &str) -> Option<Duration> {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::RETRY_AFTER,
        reqwest::header::HeaderValue::from_str(value).unwrap(),
    );
    parse_retry_after(&headers)
}

fn format_http_date(date: OffsetDateTime) -> String {
    date.format(format_description!(
        "[weekday repr:short], [day padding:zero] [month repr:short] [year] \
         [hour padding:zero]:[minute padding:zero]:[second padding:zero] GMT"
    ))
    .unwrap()
}

fn serve_once(response: impl Into<String>) -> String {
    let response = response.into();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0; 1024];
        let _ = stream.read(&mut buf);
        stream.write_all(response.as_bytes()).unwrap();
    });
    format!("http://{addr}")
}
