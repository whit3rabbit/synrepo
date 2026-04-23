use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use tempfile::tempdir;
use time::OffsetDateTime;

use synrepo::bootstrap::bootstrap;
use synrepo::config::Config;
use synrepo::core::ids::{FileNodeId, NodeId, SymbolNodeId};
use synrepo::overlay::{CommentaryEntry, CommentaryProvenance, OverlayStore};
use synrepo::pipeline::synthesis::accounting;
use synrepo::store::overlay::SqliteOverlayStore;
use synrepo::store::sqlite::SqliteGraphStore;

use super::super::commands::{synthesize_output, synthesize_status_output};
use super::support::git;

const SYNTHESIS_ENV: &[&str] = &[
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
        for key in SYNTHESIS_ENV {
            std::env::remove_var(key);
        }
        Self { _guard: guard }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for key in SYNTHESIS_ENV {
            std::env::remove_var(key);
        }
    }
}

#[test]
fn synthesize_dry_run_reports_missing_file_seed_targets() {
    let _env = EnvGuard::new();
    let (dir, repo) = setup_bootstrapped_repo("pub fn run() {}\n");

    let (stdout, stderr) = synthesize_output(&repo, Vec::new(), false, true).unwrap();
    assert!(
        stdout.contains("Synthesis dry run:"),
        "expected dry-run heading, got: {stdout}"
    );
    assert!(
        stdout.contains("repo scan if you run now: 1 file(s), 1 symbol(s) in scope"),
        "expected repo scan counts in dry-run output, got: {stdout}"
    );
    assert!(
        stdout.contains("files missing commentary: 1"),
        "expected file seed count in dry-run output, got: {stdout}"
    );
    assert!(
        stdout.contains("src/lib.rs"),
        "expected repo file path in dry-run output, got: {stdout}"
    );
    assert!(
        stderr.is_empty(),
        "dry-run should not emit live progress to stderr, got: {stderr}"
    );

    drop(dir);
}

#[test]
fn synthesize_dry_run_reports_missing_symbol_seed_candidates() {
    let _env = EnvGuard::new();
    let (dir, repo) = setup_bootstrapped_repo("pub fn run() {}\n");
    let (_file_id, symbol_id, _symbol_name, _file_hash) = lookup_ids(&repo);

    let (stdout, _) = synthesize_output(&repo, Vec::new(), false, true).unwrap();
    assert!(
        stdout.contains("symbols missing commentary: 1"),
        "expected symbol seed candidate count, got: {stdout}"
    );
    assert!(
        stdout.contains(&format!("src/lib.rs :: {}", symbol_qname(&repo, symbol_id))),
        "expected symbol target in dry-run output, got: {stdout}"
    );

    drop(dir);
}

#[test]
fn synthesize_dry_run_reports_zero_work_summary() {
    let _env = EnvGuard::new();
    let (dir, repo) = setup_bootstrapped_repo("pub fn run() {}\n");
    let (file_id, _symbol_id, _symbol_name, file_hash) = lookup_ids(&repo);
    insert_commentary(&repo, NodeId::File(file_id), &file_hash);

    let (stdout, stderr) = synthesize_output(&repo, Vec::new(), false, true).unwrap();

    assert!(
        stdout.contains("Synthesis dry run:"),
        "expected dry-run heading, got: {stdout}"
    );
    assert!(
        stdout.contains("repo scan if you run now: 1 file(s), 1 symbol(s) in scope"),
        "expected repo scan counts in dry-run output, got: {stdout}"
    );
    assert!(
        stdout.contains("max targets in this snapshot: 0"),
        "expected zero queued targets in dry-run output, got: {stdout}"
    );
    assert!(
        stdout.contains("nothing currently needs synthesis for this scope"),
        "expected no-work summary in dry-run output, got: {stdout}"
    );
    assert!(
        !stdout.contains("No commentary work planned."),
        "expected dry-run to explain checked scope instead of the old no-op line, got: {stdout}"
    );
    assert!(
        stderr.is_empty(),
        "dry-run should not emit live progress to stderr, got: {stderr}"
    );

    drop(dir);
}

#[test]
fn synthesize_emits_live_progress_and_writes_accounting() {
    let _env = EnvGuard::new();
    let (dir, repo) = setup_bootstrapped_repo("pub fn run() {}\n");
    let (file_id, symbol_id, symbol_name, file_hash) = lookup_ids(&repo);
    let (endpoint, server) = spawn_openai_compat_server(
        r#"{"choices":[{"message":{"content":"Generated commentary."}}],"usage":{"prompt_tokens":11,"completion_tokens":7}}"#,
    );
    write_local_synthesis_config(&repo, &endpoint);
    insert_commentary(&repo, NodeId::File(file_id), &file_hash);
    insert_commentary(&repo, NodeId::Symbol(symbol_id), "stale-hash");

    let (stdout, stderr) = synthesize_output(&repo, Vec::new(), false, false).unwrap();
    server.join().expect("join synthesis stub");

    assert!(
        stderr.contains("scan: checked 1/1 file(s), 1/1 symbol(s)"),
        "expected final scan line, got: {stderr}"
    );
    assert!(
        stderr.contains(
            "queued: checked 1 file(s) and 1 symbol(s) in scope; 1 item(s) need commentary (1 outdated, 0 files missing commentary, 0 symbol candidate(s))"
        ),
        "expected planned-work line, got: {stderr}"
    );
    assert!(
        stderr.contains("synthesis: refresh stale commentary and generate missing commentary"),
        "expected run intro in stderr, got: {stderr}"
    );
    assert!(
        stderr.contains(
            "api calls: yes, synrepo will send commentary requests to [local], and those requests may cost money depending on your provider billing"
        ),
        "expected api cost guidance in stderr, got: {stderr}"
    );
    assert!(
        stderr.contains(
            "output files: symbol commentary docs under .synrepo/synthesis-docs/ plus the searchable index under .synrepo/synthesis-index/"
        ),
        "expected output file guidance in stderr, got: {stderr}"
    );
    let start = stderr
        .find(&format!(
            "[1 / <= 1] [local API] update commentary for: src/lib.rs :: {symbol_name}"
        ))
        .expect("missing start progress line");
    let done = stderr
        .find(&format!(
            "[1 / <= 1] [local API] updated: src/lib.rs :: {symbol_name}"
        ))
        .expect("missing finish progress line");
    assert!(start < done, "start should appear before finish: {stderr}");
    assert!(
        stderr.contains("output file: .synrepo/synthesis-docs/symbols/"),
        "expected doc write line in stderr, got: {stderr}"
    );
    assert!(
        stderr.contains("mkdir .synrepo/synthesis-docs/symbols"),
        "expected symbol docs dir creation in stderr, got: {stderr}"
    );
    assert!(
        stderr.contains("output index: updated .synrepo/synthesis-index"),
        "expected index update line in stderr, got: {stderr}"
    );
    assert!(
        stderr.contains("provider activity: calls=1 ok=1 failed=0 budget_blocked=0 in=11 out=7"),
        "expected telemetry summary, got: {stderr}"
    );

    assert!(
        stdout.contains("commentary: 0 seeded, 1 refreshed, 0 not generated"),
        "expected final stdout summary, got: {stdout}"
    );

    let synrepo_dir = Config::synrepo_dir(&repo);
    assert!(
        accounting::log_path(&synrepo_dir).exists(),
        "expected synthesis log at {}",
        accounting::log_path(&synrepo_dir).display()
    );
    assert!(
        accounting::totals_path(&synrepo_dir).exists(),
        "expected synthesis totals at {}",
        accounting::totals_path(&synrepo_dir).display()
    );

    drop(dir);
}

#[test]
fn synthesize_plain_output_summarizes_zero_work_cleanly() {
    let _env = EnvGuard::new();
    let (dir, repo) = setup_bootstrapped_repo("pub fn run() {}\n");
    let (file_id, _symbol_id, _symbol_name, file_hash) = lookup_ids(&repo);
    insert_commentary(&repo, NodeId::File(file_id), &file_hash);

    let (_stdout, stderr) = synthesize_output(&repo, Vec::new(), false, false).unwrap();

    assert!(
        stderr.contains(
            "queued: checked 1 file(s) and 1 symbol(s) in scope; nothing currently needs commentary"
        ),
        "expected no-work queued summary, got: {stderr}"
    );
    assert!(
        stderr.contains("summary: no commentary changes were needed"),
        "expected no-work final summary, got: {stderr}"
    );
    assert!(
        !stderr.contains("outdated items: attempted=0 generated=0 not_generated=0"),
        "expected zero-work refresh summary to be suppressed, got: {stderr}"
    );
    assert!(
        !stderr.contains("missing commentary: attempted=0 generated=0 not_generated=0"),
        "expected zero-work seed summary to be suppressed, got: {stderr}"
    );

    drop(dir);
}

#[test]
fn synthesize_status_reports_pending_targets_and_summary() {
    let _env = EnvGuard::new();
    let (dir, repo) = setup_bootstrapped_repo("pub fn run() {}\n");

    let output = synthesize_status_output(&repo, Vec::new(), false).unwrap();

    assert!(
        output.contains("Synthesis status:"),
        "expected status heading, got: {output}"
    );
    assert!(
        output.contains("scope: the whole repository"),
        "expected whole-repo scope, got: {output}"
    );
    assert!(
        output.contains("repo scan if you run now: 1 file(s), 1 symbol(s) in scope"),
        "expected repo scan counts, got: {output}"
    );
    assert!(
        output.contains("files missing commentary: 1"),
        "expected queued file seed count, got: {output}"
    );
    assert!(
        output.contains("symbols missing commentary: 1"),
        "expected queued symbol seed count, got: {output}"
    );
    assert!(
        output.contains("sample pending targets"),
        "expected sample target section, got: {output}"
    );
    assert!(
        output.contains("src/lib.rs"),
        "expected repo path in sample targets, got: {output}"
    );
    assert!(
        output.contains("checked 1 file(s) and 1 symbol(s) in scope."),
        "expected checked-count summary prefix, got: {output}"
    );
    assert!(
        output.contains("would be reconsidered if you run `synrepo synthesize` now"),
        "expected summary line, got: {output}"
    );

    drop(dir);
}

fn setup_bootstrapped_repo(body: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempdir().unwrap();
    let repo = dir.path().to_path_buf();

    git(&dir, &["init", "-b", "main"]);
    git(&dir, &["config", "user.email", "test@example.com"]);
    git(&dir, &["config", "user.name", "Test"]);
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(repo.join("src/lib.rs"), body).unwrap();
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-m", "init"]);

    bootstrap(&repo, None, false).unwrap();
    (dir, repo)
}

fn lookup_ids(repo: &std::path::Path) -> (FileNodeId, SymbolNodeId, String, String) {
    let graph = SqliteGraphStore::open_existing(&Config::synrepo_dir(repo).join("graph")).unwrap();
    let file_id = graph
        .all_file_paths()
        .unwrap()
        .into_iter()
        .find_map(|(path, id)| (path == "src/lib.rs").then_some(id))
        .expect("expected src/lib.rs file node");
    let file_hash = graph
        .get_file(file_id)
        .unwrap()
        .expect("expected src/lib.rs file row")
        .content_hash;
    let (symbol_id, _file_id, qualified_name, _kind, _body_hash) = graph
        .all_symbols_summary()
        .unwrap()
        .into_iter()
        .find(|(_, candidate_file_id, qualified_name, _, _)| {
            *candidate_file_id == file_id && !qualified_name.is_empty()
        })
        .expect("expected symbol for src/lib.rs");
    (file_id, symbol_id, qualified_name, file_hash)
}

fn symbol_qname(repo: &std::path::Path, symbol_id: SymbolNodeId) -> String {
    let graph = SqliteGraphStore::open_existing(&Config::synrepo_dir(repo).join("graph")).unwrap();
    graph
        .all_symbols_summary()
        .unwrap()
        .into_iter()
        .find_map(|(id, _file_id, qname, _kind, _body_hash)| (id == symbol_id).then_some(qname))
        .expect("expected symbol qualified name")
}

fn insert_commentary(repo: &std::path::Path, node_id: NodeId, hash: &str) {
    let overlay_dir = Config::synrepo_dir(repo).join("overlay");
    let mut overlay = SqliteOverlayStore::open(&overlay_dir).unwrap();
    overlay
        .insert_commentary(CommentaryEntry {
            node_id,
            text: "Existing commentary.".to_string(),
            provenance: CommentaryProvenance {
                source_content_hash: hash.to_string(),
                pass_id: "test-commentary-v1".to_string(),
                model_identity: "test-model".to_string(),
                generated_at: OffsetDateTime::UNIX_EPOCH,
            },
        })
        .unwrap();
}

fn spawn_openai_compat_server(body: &'static str) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test synthesis server");
    let addr = listener.local_addr().expect("read test server address");
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept synthesis request");
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("set read timeout");

        let mut request = Vec::new();
        let mut buffer = [0u8; 1024];
        let mut body_len = None;
        loop {
            let read = stream.read(&mut buffer).expect("read synthesis request");
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
            .expect("write synthesis response");
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

fn write_local_synthesis_config(repo: &std::path::Path, endpoint: &str) {
    let mut config = Config::load(repo).expect("load bootstrapped config");
    config.commentary_cost_limit = 50_000;
    config.synthesis.enabled = true;
    config.synthesis.provider = Some("local".to_string());
    config.synthesis.model = Some("test-local".to_string());
    config.synthesis.local_endpoint = Some(endpoint.to_string());
    config.synthesis.local_preset = Some("custom".to_string());
    fs::write(
        Config::synrepo_dir(repo).join("config.toml"),
        toml::to_string_pretty(&config).expect("serialize config"),
    )
    .expect("write synthesis config");
}
