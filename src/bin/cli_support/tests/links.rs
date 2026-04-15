use synrepo::bootstrap::bootstrap;
use synrepo::config::{Config, Mode};
use synrepo::core::ids::{ConceptNodeId, NodeId, SymbolNodeId};
use synrepo::core::provenance::CreatedBy;
use synrepo::overlay::{
    CitedSpan, ConfidenceTier, CrossLinkProvenance, OverlayEdgeKind, OverlayEpistemic, OverlayLink,
    OverlayStore,
};
use synrepo::store::overlay::{
    compare_score_desc, format_candidate_id, FindingsFilter, SqliteOverlayStore,
};
use synrepo::store::sqlite::SqliteGraphStore;
use synrepo::structure::graph::{EdgeKind, Epistemic, GraphStore};
use tempfile::tempdir;
use time::OffsetDateTime;

use super::support::seed_graph;

/// Bring up a curated-mode repo with an overlay store opened, returning the
/// from/to node ids the four `links_accept_*` tests share.
fn setup_curated_link_env() -> (tempfile::TempDir, SqliteOverlayStore, NodeId, NodeId) {
    let repo = tempdir().unwrap();
    let ids = seed_graph(repo.path());
    std::fs::write(
        Config::synrepo_dir(repo.path()).join("config.toml"),
        "mode = \"curated\"\n",
    )
    .unwrap();

    let synrepo_dir = Config::synrepo_dir(repo.path());
    let overlay = SqliteOverlayStore::open(&synrepo_dir.join("overlay")).unwrap();
    let from = NodeId::Concept(ids.concept_id);
    let to = NodeId::Symbol(ids.symbol_id);
    (repo, overlay, from, to)
}

fn sample_link(from: NodeId, to: NodeId) -> OverlayLink {
    OverlayLink {
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
        from_content_hash: "h1".into(),
        to_content_hash: "h2".into(),
        confidence_score: 0.95,
        confidence_tier: ConfidenceTier::High,
        rationale: Some("Test rationale".into()),
        provenance: CrossLinkProvenance {
            pass_id: "test-pass".into(),
            model_identity: "test-model".into(),
            generated_at: OffsetDateTime::now_utc(),
        },
    }
}

#[test]
fn links_list_outputs_candidates() {
    let repo = tempdir().unwrap();
    bootstrap(repo.path(), Some(Mode::Auto)).unwrap();
    let synrepo_dir = Config::synrepo_dir(repo.path());
    let mut overlay = SqliteOverlayStore::open(&synrepo_dir.join("overlay")).unwrap();

    let from = NodeId::Concept(ConceptNodeId(1));
    let to = NodeId::Symbol(SymbolNodeId(2));
    overlay.insert_link(sample_link(from, to)).unwrap();

    // JSON output: assert one candidate with the expected kind and score.
    let json_text = super::super::commands::links_list_output(repo.path(), None, true).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(json_text.trim()).unwrap();
    let arr = parsed.as_array().expect("expected JSON array");
    assert_eq!(arr.len(), 1, "expected 1 candidate, got: {parsed}");
    assert_eq!(arr[0]["kind"], "references");
    assert!(
        (arr[0]["confidence_score"].as_f64().unwrap() - 0.95).abs() < 1e-6,
        "expected score 0.95, got: {}",
        arr[0]["confidence_score"]
    );

    // Human output: count line is present.
    let text = super::super::commands::links_list_output(repo.path(), None, false).unwrap();
    assert!(
        text.contains("Found 1 candidates."),
        "expected count line, got: {text}"
    );
}

#[test]
fn links_accept_blocked_in_auto_mode() {
    let repo = tempdir().unwrap();
    bootstrap(repo.path(), Some(Mode::Auto)).unwrap();

    let err = super::super::commands::links_accept(
        repo.path(),
        "concept_0000000000000001::sym_0000000000000002::references",
        None,
    )
    .unwrap_err();
    assert!(err.to_string().contains("only available in `curated` mode"));
}

#[test]
fn links_reject_blocked_in_auto_mode() {
    let repo = tempdir().unwrap();
    bootstrap(repo.path(), Some(Mode::Auto)).unwrap();

    let err = super::super::commands::links_reject(
        repo.path(),
        "concept_0000000000000001::sym_0000000000000002::references",
        None,
    )
    .unwrap_err();
    assert!(err.to_string().contains("only available in `curated` mode"));
}

#[test]
fn compare_score_desc_tolerates_nan() {
    let mut scores = [f32::NAN, 0.5, 0.9];
    scores.sort_by(|a, b| compare_score_desc(*a, *b));
    let finite: Vec<f32> = scores.iter().copied().filter(|s| !s.is_nan()).collect();
    assert_eq!(finite, vec![0.9, 0.5]);
}

#[test]
fn findings_obays_limit() {
    let repo = tempdir().unwrap();
    bootstrap(repo.path(), Some(Mode::Auto)).unwrap();
    let synrepo_dir = Config::synrepo_dir(repo.path());
    let mut overlay = SqliteOverlayStore::open(&synrepo_dir.join("overlay")).unwrap();

    let from = NodeId::Concept(ConceptNodeId(1));
    let to1 = NodeId::Symbol(SymbolNodeId(2));
    let to2 = NodeId::Symbol(SymbolNodeId(3));

    overlay.insert_link(sample_link(from, to1)).unwrap();
    overlay.insert_link(sample_link(from, to2)).unwrap();

    // Two candidates exist; limit=Some(1) must surface only one.
    let text =
        super::super::commands::findings_output(repo.path(), None, None, None, Some(1), false)
            .unwrap();
    assert!(
        text.contains("Found 1 findings."),
        "expected `Found 1 findings.` line (limit honored), got: {text}"
    );

    // Without a limit, both candidates appear. This locks down that the
    // limit value flows from CLI option to overlay query.
    let unlimited =
        super::super::commands::findings_output(repo.path(), None, None, None, None, false)
            .unwrap();
    assert!(
        unlimited.contains("Found 2 findings."),
        "expected `Found 2 findings.` line without limit, got: {unlimited}"
    );
}

#[test]
fn links_accept_writes_human_declared_edge() {
    let (repo, mut overlay, from, to) = setup_curated_link_env();
    overlay.insert_link(sample_link(from, to)).unwrap();

    let candidate_id = format_candidate_id(from, to, OverlayEdgeKind::References, "test-pass");
    super::super::commands::links_accept(repo.path(), &candidate_id, Some("reviewer-a")).unwrap();

    let synrepo_dir = Config::synrepo_dir(repo.path());
    let graph = SqliteGraphStore::open_existing(&synrepo_dir.join("graph")).unwrap();
    let edges = graph
        .outbound(from, Some(EdgeKind::References))
        .unwrap()
        .into_iter()
        .filter(|edge| edge.to == to)
        .collect::<Vec<_>>();

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].epistemic, Epistemic::HumanDeclared);
    assert_eq!(edges[0].provenance.created_by, CreatedBy::Human);
    assert_eq!(edges[0].provenance.source_revision, "curated_workflow");
    assert_eq!(edges[0].provenance.pass, "links_accept:reviewer-a");

    let audit = overlay
        .cross_link_audit_events(&from.to_string(), &to.to_string(), "references")
        .unwrap();
    assert!(audit.iter().any(|row| row.event_kind == "promoted"));
}

#[test]
fn links_accept_is_idempotent_on_replay() {
    // Two accepts must produce exactly one graph edge: the compensation path
    // depends on `insert_edge` being idempotent for crash-recovery reruns.
    let (repo, mut overlay, from, to) = setup_curated_link_env();
    overlay.insert_link(sample_link(from, to)).unwrap();

    let candidate_id = format_candidate_id(from, to, OverlayEdgeKind::References, "test-pass");
    super::super::commands::links_accept(repo.path(), &candidate_id, Some("reviewer-a")).unwrap();
    super::super::commands::links_accept(repo.path(), &candidate_id, Some("reviewer-a")).unwrap();

    let synrepo_dir = Config::synrepo_dir(repo.path());
    let graph = SqliteGraphStore::open_existing(&synrepo_dir.join("graph")).unwrap();
    let edges = graph
        .outbound(from, Some(EdgeKind::References))
        .unwrap()
        .into_iter()
        .filter(|edge| edge.to == to)
        .collect::<Vec<_>>();
    assert_eq!(edges.len(), 1, "replaying accept must not duplicate edges");
}

#[test]
fn links_accept_stale_revision_fails() {
    let (repo, mut overlay, from, to) = setup_curated_link_env();

    let mut link_v1 = sample_link(from, to);
    link_v1.provenance.pass_id = "pass-one-1234567890".into();
    overlay.insert_link(link_v1).unwrap();
    let stale_id =
        format_candidate_id(from, to, OverlayEdgeKind::References, "pass-one-1234567890");

    // upsert on insert_link replaces the row with a new pass_id.
    let mut link_v2 = sample_link(from, to);
    link_v2.provenance.pass_id = "pass-two-0987654321".into();
    overlay.insert_link(link_v2).unwrap();

    let err = super::super::commands::links_accept(repo.path(), &stale_id, Some("reviewer-a"))
        .unwrap_err();
    assert!(
        err.to_string().contains("Stale review"),
        "expected stale-review diagnostic, got: {err}"
    );
}

#[test]
fn links_accept_current_revision_succeeds() {
    let (repo, mut overlay, from, to) = setup_curated_link_env();

    let mut link = sample_link(from, to);
    link.provenance.pass_id = "fresh-pass-abcdef".into();
    overlay.insert_link(link).unwrap();

    let id = format_candidate_id(from, to, OverlayEdgeKind::References, "fresh-pass-abcdef");
    super::super::commands::links_accept(repo.path(), &id, Some("reviewer-a")).unwrap();
}

#[test]
fn links_reject_updates_candidate_state() {
    let repo = tempdir().unwrap();
    let ids = seed_graph(repo.path());
    std::fs::write(
        Config::synrepo_dir(repo.path()).join("config.toml"),
        "mode = \"curated\"\n",
    )
    .unwrap();

    let synrepo_dir = Config::synrepo_dir(repo.path());
    let mut overlay = SqliteOverlayStore::open(&synrepo_dir.join("overlay")).unwrap();
    let from = NodeId::Concept(ids.concept_id);
    let to = NodeId::Symbol(ids.symbol_id);
    overlay.insert_link(sample_link(from, to)).unwrap();

    let candidate_id = format_candidate_id(from, to, OverlayEdgeKind::References, "test-pass");
    super::super::commands::links_reject(repo.path(), &candidate_id, Some("reviewer-b")).unwrap();

    let conn =
        rusqlite::Connection::open(SqliteOverlayStore::db_path(&synrepo_dir.join("overlay")))
            .unwrap();
    let (state, reviewer): (String, Option<String>) = conn
        .query_row(
            "SELECT state, reviewer FROM cross_links WHERE from_node = ?1 AND to_node = ?2 AND kind = 'references'",
            [from.to_string(), to.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(state, "rejected");
    assert_eq!(reviewer.as_deref(), Some("reviewer-b"));
}

#[test]
fn findings_returns_below_threshold_candidates() {
    let repo = tempdir().unwrap();
    let ids = seed_graph(repo.path());
    let synrepo_dir = Config::synrepo_dir(repo.path());
    let mut overlay = SqliteOverlayStore::open(&synrepo_dir.join("overlay")).unwrap();
    let graph = SqliteGraphStore::open_existing(&synrepo_dir.join("graph")).unwrap();

    let from = NodeId::Concept(ids.concept_id);
    let to = NodeId::Symbol(ids.symbol_id);
    let mut link = sample_link(from, to);
    link.confidence_tier = ConfidenceTier::BelowThreshold;
    link.confidence_score = 0.42;
    link.from_content_hash = "hash".into();
    link.to_content_hash = "abc123".into();
    overlay.insert_link(link).unwrap();

    super::super::commands::findings(repo.path(), None, None, None, Some(10), false).unwrap();

    let findings = overlay
        .findings(&graph, &FindingsFilter::default())
        .unwrap();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].tier, ConfidenceTier::BelowThreshold);
}

fn write_curated_mode(repo: &std::path::Path) {
    std::fs::write(
        Config::synrepo_dir(repo).join("config.toml"),
        "mode = \"curated\"\n",
    )
    .unwrap();
}

/// Create an empty overlay SQLite store so the `links_accept` malformed-input
/// tests can reach the parser branch (which runs after `open_existing`).
fn ensure_overlay_initialized(repo: &std::path::Path) {
    let overlay_dir = Config::synrepo_dir(repo).join("overlay");
    let _ = SqliteOverlayStore::open(&overlay_dir).unwrap();
}

#[test]
fn links_accept_rejects_malformed_candidate_id() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());
    write_curated_mode(repo.path());
    ensure_overlay_initialized(repo.path());

    // Two `::` separators only -> 2 parts; parser requires 3 or 4.
    let err = super::super::commands::links_accept(repo.path(), "only::two", None).unwrap_err();
    assert!(
        err.to_string().contains("Invalid candidate ID format"),
        "expected format error, got: {err}"
    );
}

#[test]
fn links_accept_rejects_invalid_from_node_id() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());
    write_curated_mode(repo.path());
    ensure_overlay_initialized(repo.path());

    let err = super::super::commands::links_accept(
        repo.path(),
        "bogus_from::sym_0000000000000002::references::pass",
        None,
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("Invalid from_node"),
        "expected `Invalid from_node`, got: {err}"
    );
}

#[test]
fn links_accept_rejects_invalid_edge_kind() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());
    write_curated_mode(repo.path());
    ensure_overlay_initialized(repo.path());

    let err = super::super::commands::links_accept(
        repo.path(),
        "concept_0000000000000001::sym_0000000000000002::not_a_kind::pass",
        None,
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("Invalid edge kind"),
        "expected `Invalid edge kind`, got: {err}"
    );
}

#[test]
fn links_accept_missing_candidate_returns_error() {
    let repo = tempdir().unwrap();
    let ids = seed_graph(repo.path());
    write_curated_mode(repo.path());
    ensure_overlay_initialized(repo.path());

    let from = NodeId::Concept(ids.concept_id);
    let to = NodeId::Symbol(ids.symbol_id);
    // Well-formed candidate id, but no overlay row exists for this triple.
    let candidate_id = format_candidate_id(from, to, OverlayEdgeKind::References, "fresh-pass");

    let err = super::super::commands::links_accept(repo.path(), &candidate_id, None).unwrap_err();
    assert!(
        err.to_string().contains("Candidate not found"),
        "expected `Candidate not found`, got: {err}"
    );
}

#[test]
fn links_review_sorts_descending_and_applies_limit() {
    let repo = tempdir().unwrap();
    let ids = seed_graph(repo.path());
    write_curated_mode(repo.path());

    let synrepo_dir = Config::synrepo_dir(repo.path());
    let mut overlay = SqliteOverlayStore::open(&synrepo_dir.join("overlay")).unwrap();

    let from = NodeId::Concept(ids.concept_id);
    // Three different `to` symbols so each candidate is a distinct overlay row.
    for (idx, score) in [(2u64, 0.3f32), (3, 0.9), (4, 0.6)] {
        let mut link = sample_link(from, NodeId::Symbol(SymbolNodeId(idx)));
        link.confidence_score = score;
        link.confidence_tier = ConfidenceTier::ReviewQueue;
        overlay.insert_link(link).unwrap();
    }

    let json_text =
        super::super::commands::links_review_output(repo.path(), Some(2), true).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(json_text.trim()).unwrap();
    let arr = parsed.as_array().expect("expected JSON array");
    assert_eq!(arr.len(), 2, "limit=2 must truncate, got: {parsed}");
    let scores: Vec<f64> = arr
        .iter()
        .map(|v| v["confidence_score"].as_f64().unwrap())
        .collect();
    assert_eq!(
        scores,
        vec![0.9_f64, 0.6_f64].into_iter().collect::<Vec<_>>(),
        "expected descending sort order, got: {scores:?}"
    );
}

#[test]
fn links_accept_blocked_when_watch_running() {
    use std::process::Command;
    use synrepo::pipeline::watch::{WatchDaemonState, WatchServiceMode};

    let repo = tempdir().unwrap();
    let ids = seed_graph(repo.path());
    write_curated_mode(repo.path());

    let synrepo_dir = Config::synrepo_dir(repo.path());
    let mut overlay = SqliteOverlayStore::open(&synrepo_dir.join("overlay")).unwrap();
    let from = NodeId::Concept(ids.concept_id);
    let to = NodeId::Symbol(ids.symbol_id);
    overlay.insert_link(sample_link(from, to)).unwrap();

    // Plant a watch lease pointing at a live process so ensure_watch_not_running
    // sees Running rather than Stale.
    let mut child = Command::new("sleep").arg("5").spawn().unwrap();
    let state = WatchDaemonState {
        pid: child.id(),
        started_at: "2026-04-15T00:00:00Z".to_string(),
        mode: WatchServiceMode::Foreground,
        socket_path: synrepo_dir.join("state/watch.sock").display().to_string(),
        last_event_at: None,
        last_reconcile_at: None,
        last_reconcile_outcome: None,
        last_error: None,
        last_triggering_events: None,
    };
    let state_dir = synrepo_dir.join("state");
    std::fs::create_dir_all(&state_dir).unwrap();
    std::fs::write(
        state_dir.join("watch-daemon.json"),
        serde_json::to_string(&state).unwrap(),
    )
    .unwrap();

    let candidate_id = format_candidate_id(from, to, OverlayEdgeKind::References, "test-pass");
    let err = super::super::commands::links_accept(repo.path(), &candidate_id, None).unwrap_err();
    assert!(
        err.to_string()
            .contains("unavailable while watch service is active"),
        "expected watch-service guard error, got: {err}"
    );

    let reject_err =
        super::super::commands::links_reject(repo.path(), &candidate_id, None).unwrap_err();
    assert!(
        reject_err
            .to_string()
            .contains("unavailable while watch service is active"),
        "expected watch-service guard error from reject, got: {reject_err}"
    );

    let _ = child.kill();
    let _ = child.wait();
}
