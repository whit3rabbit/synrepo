use std::path::Path;

#[cfg(feature = "metrics-http")]
use synrepo::config::Config;

#[cfg(not(feature = "metrics-http"))]
pub(crate) fn server(_repo_root: &Path, _metrics_addr: &str) -> anyhow::Result<()> {
    anyhow::bail!("`synrepo server --metrics` requires building with `--features metrics-http`");
}

#[cfg(feature = "metrics-http")]
pub(crate) fn server(repo_root: &Path, metrics_addr: &str) -> anyhow::Result<()> {
    let synrepo_dir = Config::synrepo_dir(repo_root);
    let http = tiny_http::Server::http(metrics_addr).map_err(|error| {
        anyhow::anyhow!("failed to bind metrics server on {metrics_addr}: {error}")
    })?;
    eprintln!(
        "synrepo metrics server listening on http://{}/metrics",
        http.server_addr()
    );
    loop {
        let request = http.recv()?;
        respond(&synrepo_dir, request)?;
    }
}

#[cfg(feature = "metrics-http")]
fn respond(synrepo_dir: &Path, request: tiny_http::Request) -> anyhow::Result<()> {
    if request.method() != &tiny_http::Method::Get {
        let response = tiny_http::Response::from_string("method not allowed\n")
            .with_status_code(tiny_http::StatusCode(405));
        request.respond(response)?;
        return Ok(());
    }
    if request.url() != "/metrics" {
        let response = tiny_http::Response::from_string("not found\n")
            .with_status_code(tiny_http::StatusCode(404));
        request.respond(response)?;
        return Ok(());
    }

    let body = synrepo::pipeline::context_metrics::load(synrepo_dir)
        .map(|metrics| metrics.to_prometheus_text());
    let response = match body {
        Ok(body) => tiny_http::Response::from_string(body)
            .with_header(content_type_header()?)
            .with_status_code(tiny_http::StatusCode(200)),
        Err(error) => tiny_http::Response::from_string(format!("metrics unavailable: {error}\n"))
            .with_status_code(tiny_http::StatusCode(500)),
    };
    request.respond(response)?;
    Ok(())
}

#[cfg(feature = "metrics-http")]
fn content_type_header() -> anyhow::Result<tiny_http::Header> {
    tiny_http::Header::from_bytes(
        b"Content-Type".as_slice(),
        b"text/plain; version=0.0.4; charset=utf-8".as_slice(),
    )
    .map_err(|_| anyhow::anyhow!("failed to build metrics content-type header"))
}

#[cfg(all(test, feature = "metrics-http"))]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::thread;

    use synrepo::pipeline::context_metrics::{self, ContextMetrics};

    use super::*;

    #[test]
    fn metrics_endpoint_serves_prometheus_text() {
        let tempdir = tempfile::tempdir().unwrap();
        let synrepo_dir = Config::synrepo_dir(tempdir.path());
        let metrics = ContextMetrics {
            cards_served_total: 2,
            card_tokens_total: 120,
            raw_file_tokens_total: 400,
            estimated_tokens_saved_total: 280,
            stale_responses_total: 1,
            ..ContextMetrics::default()
        };
        context_metrics::save(&synrepo_dir, &metrics).unwrap();

        let http = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = http.server_addr().to_string();
        let handle = thread::spawn(move || {
            let request = http.recv().unwrap();
            respond(&synrepo_dir, request).unwrap();
        });

        let mut stream = TcpStream::connect(addr).unwrap();
        stream
            .write_all(b"GET /metrics HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        handle.join().unwrap();

        assert!(response.starts_with("HTTP/1.1 200"));
        let body = response.split("\r\n\r\n").nth(1).unwrap_or_default();
        assert_prometheus_parseable(body);
        assert!(body.contains("synrepo_cards_served_total 2"));
        assert!(body.contains("synrepo_stale_responses_total 1"));
    }

    fn assert_prometheus_parseable(body: &str) {
        for line in body.lines().filter(|line| !line.is_empty()) {
            if line.starts_with('#') {
                assert!(
                    line.starts_with("# HELP ") || line.starts_with("# TYPE "),
                    "unexpected comment line: {line}"
                );
                continue;
            }
            let mut fields = line.split_whitespace();
            let metric = fields.next().expect("metric name");
            let value = fields.next().expect("metric value");
            assert!(
                metric.starts_with("synrepo_"),
                "metric must use synrepo prefix: {metric}"
            );
            value
                .parse::<u64>()
                .expect("metric value must parse as u64");
            assert!(fields.next().is_none(), "unexpected extra metric fields");
        }
    }
}
