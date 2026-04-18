use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    time::{Duration, Instant},
};

use synrepo::bootstrap::bootstrap;
use synrepo::config::{Config, Mode};
use synrepo::core::ids::{ConceptNodeId, FileNodeId, SymbolNodeId};
use synrepo::core::provenance::{CreatedBy, Provenance, SourceRef};
use synrepo::overlay::{
    CitedSpan, ConfidenceTier, CrossLinkProvenance, OverlayEdgeKind, OverlayEpistemic, OverlayLink,
    OverlayStore,
};
use synrepo::pipeline::export::MANIFEST_FILENAME;
use synrepo::pipeline::writer::{
    hold_writer_flock_with_ownership, writer_lock_path, TestFlockHolder, WriterOwnership,
};
use synrepo::store::overlay::{format_candidate_id, SqliteOverlayStore};
use synrepo::store::sqlite::SqliteGraphStore;
use synrepo::structure::graph::{
    ConceptNode, EdgeKind, Epistemic, FileNode, GraphStore, SymbolKind, SymbolNode, Visibility,
};
use synrepo::NodeId;
use tempfile::{tempdir, TempDir};
use time::OffsetDateTime;

pub(crate) struct SeededLinkRepo {
    pub(crate) _dir: TempDir,
    pub(crate) repo: PathBuf,
    pub(crate) from: NodeId,
    pub(crate) to: NodeId,
    pub(crate) candidate_id: String,
}

pub(crate) fn init_repo() -> TempDir {
    let repo = tempdir().expect("tempdir");
    fs::create_dir_all(repo.path().join("src")).expect("create src");
    fs::write(repo.path().join("src/lib.rs"), "pub fn hello() {}\n").expect("write src/lib.rs");
    bootstrap(repo.path(), None).expect("bootstrap repo");
    repo
}

pub(crate) fn setup_curated_link_repo(pass_id: &str) -> SeededLinkRepo {
    let dir = tempdir().expect("tempdir");
    let repo = dir.path().to_path_buf();
    fs::create_dir_all(repo.join("src")).expect("create src");
    fs::write(repo.join("src/lib.rs"), "pub fn hello() {}\n").expect("write src/lib.rs");
    bootstrap(&repo, Some(Mode::Curated)).expect("bootstrap curated repo");

    let synrepo_dir = synrepo_dir(&repo);
    let graph_dir = synrepo_dir.join("graph");
    let overlay_dir = synrepo_dir.join("overlay");

    let file_id = FileNodeId(0x42);
    let symbol_id = SymbolNodeId(0x24);
    let concept_id = ConceptNodeId(0x99);
    let from = NodeId::Concept(concept_id);
    let to = NodeId::File(file_id);
    let target_path = "tests/fixtures/soak_target.rs";

    let mut graph = SqliteGraphStore::open(&graph_dir).expect("open graph");
    graph.begin().expect("begin graph tx");
    graph
        .upsert_file(FileNode {
            id: file_id,
            path: target_path.to_string(),
            path_history: Vec::new(),
            content_hash: "abc123".to_string(),
            size_bytes: 64,
            language: Some("rust".to_string()),
            inline_decisions: Vec::new(),
            last_observed_rev: None,
            epistemic: Epistemic::ParserObserved,
            provenance: sample_provenance("parse_code", target_path),
        })
        .expect("upsert file");
    graph
        .upsert_symbol(SymbolNode {
            id: symbol_id,
            file_id,
            qualified_name: "synrepo::hello".to_string(),
            display_name: "hello".to_string(),
            kind: SymbolKind::Function,
            visibility: Visibility::Public,
            body_byte_range: (0, 16),
            body_hash: "bodyhash".to_string(),
            signature: Some("pub fn hello()".to_string()),
            doc_comment: None,
            first_seen_rev: None,
            last_modified_rev: None,
            last_observed_rev: None,
            retired_at_rev: None,
            epistemic: Epistemic::ParserObserved,
            provenance: sample_provenance("parse_code", target_path),
        })
        .expect("upsert symbol");
    graph
        .upsert_concept(ConceptNode {
            id: concept_id,
            path: "docs/adr/0001-link.md".to_string(),
            title: "Link target".to_string(),
            aliases: vec!["link-target".to_string()],
            summary: Some("Soak-test concept.".to_string()),
            status: None,
            decision_body: None,
            last_observed_rev: None,
            epistemic: Epistemic::HumanDeclared,
            provenance: sample_provenance("parse_prose", "docs/adr/0001-link.md"),
        })
        .expect("upsert concept");
    graph.commit().expect("commit graph tx");

    let mut overlay = SqliteOverlayStore::open(&overlay_dir).expect("open overlay");
    overlay
        .insert_link(OverlayLink {
            from,
            to,
            kind: OverlayEdgeKind::References,
            epistemic: OverlayEpistemic::MachineAuthoredHighConf,
            source_spans: vec![CitedSpan {
                artifact: from,
                normalized_text: "source".into(),
                verified_at_offset: 0,
                lcs_ratio: 1.0,
            }],
            target_spans: vec![CitedSpan {
                artifact: to,
                normalized_text: "target".into(),
                verified_at_offset: 0,
                lcs_ratio: 1.0,
            }],
            from_content_hash: "from-hash".into(),
            to_content_hash: "to-hash".into(),
            confidence_score: 0.95,
            confidence_tier: ConfidenceTier::High,
            rationale: Some("soak candidate".into()),
            provenance: CrossLinkProvenance {
                pass_id: pass_id.to_string(),
                model_identity: "soak-test".into(),
                generated_at: OffsetDateTime::UNIX_EPOCH,
            },
        })
        .expect("insert link");

    SeededLinkRepo {
        _dir: dir,
        repo: repo.clone(),
        from,
        to,
        candidate_id: format_candidate_id(from, to, OverlayEdgeKind::References, pass_id),
    }
}

pub(crate) fn command(repo: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_synrepo"));
    command.arg("--repo").arg(repo);
    command
}

pub(crate) fn run(repo: &Path, args: &[&str]) -> Output {
    command(repo).args(args).output().expect("run synrepo")
}

pub(crate) fn run_with_env(repo: &Path, args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut command = command(repo);
    command.args(args);
    for (key, value) in envs {
        command.env(key, value);
    }
    command.output().expect("run synrepo with env")
}

pub(crate) fn run_ok(repo: &Path, args: &[&str]) -> String {
    assert_success(run(repo, args))
}

pub(crate) fn assert_success(output: Output) -> String {
    assert!(
        output.status.success(),
        "command failed: stdout={}, stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

pub(crate) fn assert_failure_contains(output: Output, needle: &str) -> String {
    assert!(
        !output.status.success(),
        "command unexpectedly succeeded: stdout={}, stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        stderr.contains(needle),
        "stderr missing `{needle}`: {stderr}"
    );
    stderr
}

pub(crate) fn wait_for(mut condition: impl FnMut() -> bool, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if condition() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("timed out waiting for condition");
}

pub(crate) fn start_watch_daemon(repo: &Path) {
    let _ = run_ok(repo, &["watch", "--daemon"]);
    wait_for(
        || {
            let output = run(repo, &["watch", "status"]);
            output.status.success()
                && String::from_utf8_lossy(&output.stdout).contains("state:        running")
        },
        Duration::from_secs(5),
    );
}

pub(crate) fn stop_watch(repo: &Path) {
    let _ = run_ok(repo, &["watch", "stop"]);
}

pub(crate) fn watch_status(repo: &Path) -> String {
    assert_success(run(repo, &["watch", "status"]))
}

pub(crate) fn read_watch_pid(repo: &Path) -> u32 {
    let path = synrepo_dir(repo).join("state/watch-daemon.json");
    let raw = fs::read_to_string(path).expect("read watch-daemon.json");
    serde_json::from_str::<serde_json::Value>(&raw)
        .expect("parse watch-daemon.json")
        .get("pid")
        .and_then(|value| value.as_u64())
        .expect("watch pid")
        .try_into()
        .expect("pid fits u32")
}

pub(crate) fn kill_pid(pid: u32) {
    let status = Command::new("kill")
        .args(["-9", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("kill -9");
    assert!(status.success(), "failed to kill pid {pid}");
}

pub(crate) fn hold_foreign_writer_lock(repo: &Path) -> (Child, TestFlockHolder, u32) {
    let child = Command::new("sleep")
        .arg("30")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn sleep");
    let pid = child.id();
    let holder = hold_writer_flock_with_ownership(
        &writer_lock_path(&synrepo_dir(repo)),
        &WriterOwnership {
            pid,
            acquired_at: "2099-01-01T00:00:00Z".to_string(),
        },
    );
    (child, holder, pid)
}

pub(crate) fn write_upgrade_drift(repo: &Path) {
    let synrepo_dir = synrepo_dir(repo);
    let updated = Config {
        roots: vec!["src".to_string()],
        ..Config::load(repo).expect("load config")
    };
    fs::write(
        synrepo_dir.join("config.toml"),
        toml::to_string_pretty(&updated).expect("serialize config"),
    )
    .expect("write config");
}

pub(crate) fn synrepo_dir(repo: &Path) -> PathBuf {
    Config::synrepo_dir(repo)
}

pub(crate) fn export_manifest_path(repo: &Path) -> PathBuf {
    repo.join("synrepo-context").join(MANIFEST_FILENAME)
}

pub(crate) fn compact_state_path(repo: &Path) -> PathBuf {
    synrepo_dir(repo).join("state/compact-state.json")
}

pub(crate) fn reconcile_state_path(repo: &Path) -> PathBuf {
    synrepo_dir(repo).join("state/reconcile-state.json")
}

pub(crate) fn read_optional(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok()
}

pub(crate) fn candidate_state(repo: &Path, fixture: &SeededLinkRepo) -> String {
    let conn = rusqlite::Connection::open(SqliteOverlayStore::db_path(
        &synrepo_dir(repo).join("overlay"),
    ))
    .expect("open overlay db");
    conn.query_row(
        "SELECT state FROM cross_links WHERE from_node = ?1 AND to_node = ?2 AND kind = 'references'",
        [fixture.from.to_string(), fixture.to.to_string()],
        |row| row.get(0),
    )
    .expect("candidate state")
}

pub(crate) fn edge_count(repo: &Path, fixture: &SeededLinkRepo) -> usize {
    let graph =
        SqliteGraphStore::open_existing(&synrepo_dir(repo).join("graph")).expect("open graph");
    graph
        .outbound(fixture.from, Some(EdgeKind::References))
        .expect("load outbound edges")
        .into_iter()
        .filter(|edge| edge.to == fixture.to)
        .count()
}

fn sample_provenance(pass: &str, path: &str) -> Provenance {
    Provenance {
        created_at: OffsetDateTime::UNIX_EPOCH,
        source_revision: "deadbeef".to_string(),
        created_by: CreatedBy::StructuralPipeline,
        pass: pass.to_string(),
        source_artifacts: vec![SourceRef {
            file_id: None,
            path: path.to_string(),
            content_hash: "hash".to_string(),
        }],
    }
}
