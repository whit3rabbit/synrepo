//! Readiness-gate tests for `synrepo mcp`. These exist to catch regressions
//! where the server would happily accept clients against an unready store and
//! only surface the failure per tool call.

use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use tempfile::tempdir;

use synrepo::bootstrap::bootstrap;
use synrepo::config::Config;
use synrepo::pipeline::explain::{accounting, telemetry};
use synrepo::store::compatibility::snapshot_path;
use synrepo::store::overlay::SqliteOverlayStore;

use super::support::git;
use crate::prepare_mcp_state;

const EXPLAIN_ENV: &[&str] = &[
    "SYNREPO_LLM_ENABLED",
    "SYNREPO_LLM_PROVIDER",
    "SYNREPO_LLM_MODEL",
    "SYNREPO_LLM_LOCAL_ENDPOINT",
    "ANTHROPIC_API_KEY",
    "SYNREPO_ANTHROPIC_API_KEY",
    "OPENAI_API_KEY",
    "GEMINI_API_KEY",
    "OPENROUTER_API_KEY",
];

static ENV_LOCK: Mutex<()> = Mutex::new(());

struct EnvGuard {
    _guard: std::sync::MutexGuard<'static, ()>,
}

impl EnvGuard {
    fn new() -> Self {
        let guard = ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for key in EXPLAIN_ENV {
            std::env::remove_var(key);
        }
        Self { _guard: guard }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for key in EXPLAIN_ENV {
            std::env::remove_var(key);
        }
    }
}

fn setup_bootstrapped_repo() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempdir().unwrap();
    let repo = dir.path();

    git(&dir, &["init", "-b", "main"]);
    git(&dir, &["config", "user.email", "test@example.com"]);
    git(&dir, &["config", "user.name", "Test"]);
    fs::write(repo.join("lib.rs"), "fn main() {}").unwrap();
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-m", "init"]);

    bootstrap(repo, None, false).unwrap();

    let repo_path = repo.to_path_buf();
    (dir, repo_path)
}

fn spawn_openai_compat_server(body: &'static str) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test explain server");
    let addr = listener.local_addr().expect("read test server address");
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept explain request");
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("set read timeout");

        let mut request = Vec::new();
        let mut buffer = [0u8; 1024];
        let mut body_len = None;
        loop {
            let read = stream.read(&mut buffer).expect("read explain request");
            if read == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..read]);
            if let Some(header_end) = find_header_end(&request) {
                let content_length = parse_content_length(&request[..header_end]);
                body_len = Some(content_length);
                if request.len() >= header_end + content_length {
                    break;
                }
            }
        }

        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream
            .write_all(response.as_bytes())
            .expect("write explain response");
        if let Some(expected) = body_len {
            assert!(expected > 0, "expected non-empty JSON request body");
        }
    });

    (format!("http://{addr}/v1/chat/completions"), handle)
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|idx| idx + 4)
}

fn parse_content_length(headers: &[u8]) -> usize {
    let headers = String::from_utf8_lossy(headers);
    headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            if name.eq_ignore_ascii_case("content-length") {
                value.trim().parse::<usize>().ok()
            } else {
                None
            }
        })
        .unwrap_or(0)
}

fn write_local_explain_config(repo: &std::path::Path, endpoint: &str) {
    let mut config = Config::load(repo).expect("load bootstrapped config");
    config.commentary_cost_limit = 50_000;
    config.explain.enabled = true;
    config.explain.provider = Some("local".to_string());
    config.explain.model = Some("test-local".to_string());
    config.explain.local_endpoint = Some(endpoint.to_string());
    config.explain.local_preset = Some("custom".to_string());
    fs::write(
        Config::synrepo_dir(repo).join("config.toml"),
        toml::to_string_pretty(&config).expect("serialize config"),
    )
    .expect("write explain config");
}

fn materialize_overlay(repo: &std::path::Path) {
    let overlay_dir = Config::synrepo_dir(repo).join("overlay");
    drop(SqliteOverlayStore::open(&overlay_dir).expect("create overlay store"));
}

#[test]
fn prepare_state_succeeds_on_fresh_bootstrap() {
    let (dir, repo) = setup_bootstrapped_repo();
    prepare_mcp_state(&repo).expect("fresh bootstrap must pass the MCP readiness gate");
    drop(dir);
}

#[test]
fn prepare_state_fails_when_compatibility_snapshot_is_missing() {
    let (dir, repo) = setup_bootstrapped_repo();
    let synrepo_dir = Config::synrepo_dir(&repo);

    // Same trick upgrade's own tests use: a materialized canonical store with
    // no compatibility snapshot is a Block action, which must fail the gate
    // instead of letting MCP come up and surface the error per tool call.
    let snap = snapshot_path(&synrepo_dir);
    fs::remove_file(&snap).expect("snapshot must exist after bootstrap");

    let err = match prepare_mcp_state(&repo) {
        Err(e) => e,
        Ok(_) => panic!("blocking compatibility must fail the gate"),
    };
    let msg = err.to_string();
    assert!(
        msg.contains("synrepo upgrade"),
        "fail-fast message must point users at `synrepo upgrade`, got: {msg}"
    );
    drop(dir);
}

#[test]
fn refresh_commentary_via_mcp_records_explain_accounting() {
    let _env = EnvGuard::new();
    let (dir, repo) = setup_bootstrapped_repo();
    let (endpoint, server) = spawn_openai_compat_server(
        r#"{"choices":[{"message":{"content":"Generated commentary."}}],"usage":{"prompt_tokens":11,"completion_tokens":7}}"#,
    );
    write_local_explain_config(&repo, &endpoint);
    materialize_overlay(&repo);

    let state = prepare_mcp_state(&repo).expect("MCP state should load with explain enabled");
    let synrepo_dir = Config::synrepo_dir(&repo);
    assert_eq!(
        telemetry::synrepo_dir().as_deref(),
        Some(synrepo_dir.as_path())
    );

    let output =
        synrepo::surface::mcp::cards::handle_refresh_commentary(&state, "main".to_string());
    let json: serde_json::Value =
        serde_json::from_str(&output).expect("refresh_commentary should return JSON");
    assert_eq!(
        json["status"], "refreshed",
        "unexpected MCP output: {output}"
    );
    assert_eq!(json["commentary"], "Generated commentary.");
    let node_id = json["node_id"]
        .as_str()
        .expect("refresh_commentary should return node_id");
    let doc_path = synrepo_dir
        .join("explain-docs")
        .join("symbols")
        .join(format!("{node_id}.md"));
    assert!(
        doc_path.exists(),
        "refresh_commentary should materialize advisory docs at {}",
        doc_path.display()
    );

    let docs_output = synrepo::surface::mcp::docs::handle_docs_search(
        &state,
        "Generated commentary.".to_string(),
        10,
    );
    let docs_json: serde_json::Value =
        serde_json::from_str(&docs_output).expect("docs_search should return JSON");
    let results = docs_json["results"]
        .as_array()
        .expect("docs_search should return a results array");
    assert_eq!(
        results.len(),
        1,
        "unexpected docs search output: {docs_output}"
    );
    assert_eq!(results[0]["node_id"], node_id);
    assert_eq!(results[0]["source_store"], "overlay");
    assert_eq!(results[0]["content"], "Generated commentary.");

    server.join().expect("join explain stub");

    let log = fs::read_to_string(accounting::log_path(&synrepo_dir))
        .expect("refresh_commentary should write explain log");
    assert!(
        log.contains("\"provider\":\"local\"") && log.contains("\"outcome\":\"success\""),
        "expected successful local explain log entry, got: {log}"
    );

    let totals: accounting::ExplainTotals = serde_json::from_slice(
        &fs::read(accounting::totals_path(&synrepo_dir))
            .expect("refresh_commentary should write explain totals"),
    )
    .expect("totals file should parse");
    assert_eq!(totals.calls, 1);
    assert_eq!(totals.failures, 0);
    assert_eq!(totals.budget_blocked, 0);
    assert_eq!(totals.input_tokens, 11);
    assert_eq!(totals.output_tokens, 7);
    assert_eq!(totals.usd_cost, 0.0);
    assert!(!totals.any_unpriced);

    drop(dir);
}

#[test]
fn mcp_source_registers_docs_search_tool() {
    let source = fs::read_to_string("src/bin/cli_support/commands/mcp.rs")
        .expect("read MCP registration source");
    assert!(
        source.contains("name = \"synrepo_docs_search\""),
        "MCP registration must include synrepo_docs_search"
    );
}

#[test]
fn prepare_state_fails_on_uninitialized_repo() {
    // Config::load falls back to ~/.synrepo/config.toml; redirect HOME to an
    // empty tempdir under the shared lock so the developer's real user-scoped
    // config can't satisfy the load and hide the uninitialized state.
    let _lock =
        synrepo::test_support::global_test_lock(synrepo::config::test_home::HOME_ENV_TEST_LOCK);
    let home = tempdir().unwrap();
    let _home_guard = synrepo::config::test_home::HomeEnvGuard::redirect_to(home.path());

    let dir = tempdir().unwrap();
    let repo = dir.path().to_path_buf();
    // Deliberately no `synrepo init` / `bootstrap` — the server must refuse
    // to start rather than serving a tool that then trips over a missing
    // config.
    let err = match prepare_mcp_state(&repo) {
        Err(e) => e,
        Ok(_) => panic!("uninitialized repo must fail the gate"),
    };
    let msg = err.to_string();
    assert!(
        msg.contains("synrepo init"),
        "fail-fast message must point users at `synrepo init`, got: {msg}"
    );
    drop(dir);
}
