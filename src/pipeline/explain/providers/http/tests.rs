use super::*;
use crate::pipeline::explain::telemetry::UsageSource;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

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

fn serve_once(response: &'static str) -> String {
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
