use std::process::Command;

use synrepo::bootstrap::{bootstrap, BootstrapReport};
use synrepo::config::Config;
use synrepo::core::ids::{ConceptNodeId, EdgeId, FileNodeId, SymbolNodeId};
use synrepo::core::provenance::{CreatedBy, Provenance, SourceRef};
use synrepo::overlay::{
    CitedSpan, ConfidenceTier, CrossLinkProvenance, OverlayEdgeKind, OverlayEpistemic, OverlayLink,
    OverlayStore,
};
use synrepo::store::overlay::SqliteOverlayStore;
use synrepo::store::sqlite::SqliteGraphStore;
use synrepo::structure::graph::{
    ConceptNode, EdgeKind, Epistemic, FileNode, GraphStore, SymbolKind, SymbolNode, Visibility,
};
use synrepo::NodeId;
use time::OffsetDateTime;

/// Canonicalize `path` and strip the Windows verbatim (`\\?\`) prefix.
///
/// `std::fs::canonicalize` returns paths prefixed with `\\?\` on Windows.
/// `agent-config` 0.1 walks parent directories during HOME-relative MCP
/// installs and fails on `\\?\C:` (the bare verbatim drive root) with
/// "Incorrect function (os error 1)". Stripping the prefix yields a
/// drive-letter path that the crate handles correctly. No-op on other OSes.
pub(crate) fn canonicalize_no_verbatim(path: &std::path::Path) -> std::path::PathBuf {
    let canonical = std::fs::canonicalize(path).expect("canonicalize tempdir path");
    #[cfg(windows)]
    {
        let s = canonical.as_os_str().to_string_lossy().into_owned();
        if let Some(rest) = s.strip_prefix(r"\\?\") {
            return std::path::PathBuf::from(rest);
        }
    }
    canonical
}

pub(super) fn git(repo: &tempfile::TempDir, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?} failed: stdout={}, stderr={}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

pub(super) fn git_stdout(repo: &tempfile::TempDir, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?} failed: stdout={}, stderr={}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

pub(super) fn git_with_author(repo: &tempfile::TempDir, args: &[&str], author: &str, email: &str) {
    let output = Command::new("git")
        .env("GIT_AUTHOR_NAME", author)
        .env("GIT_AUTHOR_EMAIL", email)
        .env("GIT_COMMITTER_NAME", author)
        .env("GIT_COMMITTER_EMAIL", email)
        .args(args)
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?} failed: stdout={}, stderr={}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

pub(super) fn bootstrap_isolated(
    repo_root: &std::path::Path,
    mode: Option<synrepo::config::Mode>,
    update_gitignore: bool,
) -> anyhow::Result<BootstrapReport> {
    let _lock =
        synrepo::test_support::global_test_lock(synrepo::config::test_home::HOME_ENV_TEST_LOCK);
    let home = tempfile::tempdir().unwrap();
    let canonical_home = canonicalize_no_verbatim(home.path());
    let _guard = synrepo::config::test_home::HomeEnvGuard::redirect_to(&canonical_home);
    bootstrap(repo_root, mode, update_gitignore)
}

pub(super) fn synthetic_watch_state(
    mode: synrepo::pipeline::watch::WatchServiceMode,
    pid: u32,
    started_at: &str,
    endpoint: String,
) -> synrepo::pipeline::watch::WatchDaemonState {
    synrepo::pipeline::watch::WatchDaemonState {
        pid,
        started_at: started_at.to_string(),
        mode,
        control_endpoint: endpoint,
        last_event_at: None,
        last_reconcile_at: None,
        last_reconcile_outcome: None,
        last_error: None,
        last_triggering_events: None,
        auto_sync_enabled: false,
        auto_sync_running: false,
        auto_sync_paused: false,
        auto_sync_last_started_at: None,
        auto_sync_last_finished_at: None,
        auto_sync_last_outcome: None,
        embedding_index_stale: false,
        embedding_running: false,
        embedding_last_started_at: None,
        embedding_last_finished_at: None,
        embedding_last_outcome: None,
        embedding_last_error: None,
        embedding_progress_phase: None,
        embedding_progress_current: None,
        embedding_progress_total: None,
        embedding_next_retry_at: None,
    }
}

pub(super) struct SeededGraphIds {
    pub(super) file_id: FileNodeId,
    pub(super) symbol_id: SymbolNodeId,
    pub(super) concept_id: ConceptNodeId,
}

pub(super) fn seed_graph(repo_root: &std::path::Path) -> SeededGraphIds {
    bootstrap_isolated(repo_root, None, false).unwrap();

    let graph_dir = Config::synrepo_dir(repo_root).join("graph");
    let mut store = SqliteGraphStore::open(&graph_dir).unwrap();
    let file_id = FileNodeId(0x42);
    let symbol_id = SymbolNodeId(0x24);
    let concept_id = ConceptNodeId(0x99);

    store.begin().unwrap();
    store
        .upsert_file(FileNode {
            id: file_id,
            root_id: "primary".to_string(),
            path: "src/lib.rs".to_string(),
            path_history: vec!["src/old_lib.rs".to_string()],
            content_hash: "abc123".to_string(),
            content_sample_hashes: Vec::new(),
            size_bytes: 128,
            language: Some("rust".to_string()),
            inline_decisions: Vec::new(),
            last_observed_rev: None,
            epistemic: Epistemic::ParserObserved,
            provenance: sample_provenance("parse_code", "src/lib.rs"),
        })
        .unwrap();
    store
        .upsert_symbol(SymbolNode {
            id: symbol_id,
            file_id,
            qualified_name: "synrepo::lib".to_string(),
            display_name: "lib".to_string(),
            kind: SymbolKind::Module,
            visibility: Visibility::Public,
            body_byte_range: (0, 64),
            body_hash: "def456".to_string(),
            signature: Some("pub mod lib".to_string()),
            doc_comment: None,
            first_seen_rev: None,
            last_modified_rev: None,
            last_observed_rev: None,
            retired_at_rev: None,
            epistemic: Epistemic::ParserObserved,
            provenance: sample_provenance("parse_code", "src/lib.rs"),
        })
        .unwrap();
    store
        .upsert_concept(ConceptNode {
            id: concept_id,
            path: "docs/adr/0001-graph.md".to_string(),
            title: "Graph Storage".to_string(),
            aliases: vec!["canonical-graph".to_string()],
            summary: Some("Why the graph stays observed-only.".to_string()),
            status: None,
            decision_body: None,
            last_observed_rev: None,
            epistemic: Epistemic::HumanDeclared,
            provenance: sample_provenance("parse_prose", "docs/adr/0001-graph.md"),
        })
        .unwrap();
    store
        .insert_edge(synrepo::structure::graph::Edge {
            id: EdgeId(0x77),
            from: NodeId::File(file_id),
            to: NodeId::Symbol(symbol_id),
            kind: EdgeKind::Defines,
            owner_file_id: None,
            last_observed_rev: None,
            retired_at_rev: None,
            epistemic: Epistemic::ParserObserved,
            provenance: sample_provenance("resolve_edges", "src/lib.rs"),
        })
        .unwrap();
    store
        .insert_edge(synrepo::structure::graph::Edge {
            id: EdgeId(0x78),
            from: NodeId::Concept(concept_id),
            to: NodeId::File(file_id),
            kind: EdgeKind::Governs,
            owner_file_id: None,
            last_observed_rev: None,
            retired_at_rev: None,
            epistemic: Epistemic::HumanDeclared,
            provenance: sample_provenance("parse_prose", "docs/adr/0001-graph.md"),
        })
        .unwrap();
    store.commit().unwrap();
    let mut snapshot_graph = synrepo::structure::graph::Graph::from_store(&store).unwrap();
    snapshot_graph.snapshot_epoch = 1;
    synrepo::structure::graph::snapshot::publish(repo_root, snapshot_graph);

    SeededGraphIds {
        file_id,
        symbol_id,
        concept_id,
    }
}

/// Seed `n` synthetic cross-link candidates into the repo's overlay store.
///
/// Each row gets a unique `(from, to)` pair so `ON CONFLICT` upserts do not
/// collapse them. Callers are responsible for `bootstrap()` if the test also
/// needs a graph; the overlay directory is created on demand.
pub(super) fn seed_overlay_candidates(repo_root: &std::path::Path, n: u64) {
    let overlay_dir = Config::synrepo_dir(repo_root).join("overlay");
    let mut overlay = SqliteOverlayStore::open(&overlay_dir).unwrap();

    for i in 0..n {
        let from = NodeId::Concept(ConceptNodeId((i + 1) as u128));
        let to = NodeId::File(FileNodeId((i + 1 + 10_000) as u128));
        let link = OverlayLink {
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
            from_content_hash: format!("h-from-{i}"),
            to_content_hash: format!("h-to-{i}"),
            confidence_score: 0.5 + (i as f32 % 50.0) / 100.0,
            confidence_tier: if i % 2 == 0 {
                ConfidenceTier::High
            } else {
                ConfidenceTier::ReviewQueue
            },
            rationale: None,
            provenance: CrossLinkProvenance {
                pass_id: "scale-test".into(),
                model_identity: "scale-test-model".into(),
                generated_at: OffsetDateTime::UNIX_EPOCH,
            },
        };
        overlay.insert_link(link).unwrap();
    }
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
