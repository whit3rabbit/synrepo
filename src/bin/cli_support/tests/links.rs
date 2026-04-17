use synrepo::bootstrap::bootstrap;
use synrepo::config::{Config, Mode};
use synrepo::core::ids::{ConceptNodeId, EdgeId, NodeId, SymbolNodeId};
use synrepo::core::provenance::CreatedBy;
use synrepo::overlay::{
    CitedSpan, ConfidenceTier, CrossLinkProvenance, OverlayEdgeKind, OverlayEpistemic, OverlayLink,
    OverlayStore,
};
use synrepo::store::overlay::{
    compare_score_desc, format_candidate_id, FindingsFilter, SqliteOverlayStore,
};
use synrepo::store::sqlite::SqliteGraphStore;
use synrepo::structure::graph::{Edge, EdgeKind, Epistemic, GraphStore};
use tempfile::tempdir;
use time::OffsetDateTime;

use super::super::commands::{CommitArgs, LinksCommitStore, RealLinksStore};
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
    let json_text =
        super::super::commands::links_list_output(repo.path(), None, None, true).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(json_text.trim()).unwrap();
    let arr = parsed.as_array().expect("expected JSON array");
    assert_eq!(arr.len(), 1, "expected 1 candidate, got: {parsed}");
    assert_eq!(arr[0]["kind"], "references");
    assert!(
        (arr[0]["confidence_score"].as_f64().unwrap() - 0.95).abs() < 1e-6,
        "expected score 0.95, got: {}",
        arr[0]["confidence_score"]
    );

    // Human output: count line is present. Row count (1) is below the default
    // cap, so the message stays on the "Found N candidates." branch.
    let text = super::super::commands::links_list_output(repo.path(), None, None, false).unwrap();
    assert!(
        text.contains("Found 1 candidates."),
        "expected count line, got: {text}"
    );
}

fn insert_n_candidates(overlay: &mut SqliteOverlayStore, from: NodeId, n: u64) {
    for i in 0..n {
        let to = NodeId::Symbol(SymbolNodeId(1000 + i));
        overlay.insert_link(sample_link(from, to)).unwrap();
    }
}

/// Default `links list` invocation (no `--limit`) must cap at 50 rows, even
/// when the overlay store contains more. This prevents unbounded RAM use and
/// output flooding for repos with large candidate sets.
#[test]
fn links_list_default_limit_caps_at_50() {
    let repo = tempdir().unwrap();
    bootstrap(repo.path(), Some(Mode::Auto)).unwrap();
    let synrepo_dir = Config::synrepo_dir(repo.path());
    let mut overlay = SqliteOverlayStore::open(&synrepo_dir.join("overlay")).unwrap();

    let from = NodeId::Concept(ConceptNodeId(1));
    insert_n_candidates(&mut overlay, from, 75);

    let json = super::super::commands::links_list_output(repo.path(), None, None, true).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(json.trim()).unwrap();
    let arr = parsed.as_array().expect("expected JSON array");
    assert_eq!(arr.len(), 50, "default limit must cap JSON output at 50");

    let text = super::super::commands::links_list_output(repo.path(), None, None, false).unwrap();
    assert!(
        text.contains("Showing 50 candidates (capped at 50"),
        "default-cap path must surface the cap hint, got: {text}"
    );
}

/// `--limit 0` disables the cap and materializes the full candidate set.
#[test]
fn links_list_explicit_zero_returns_all() {
    let repo = tempdir().unwrap();
    bootstrap(repo.path(), Some(Mode::Auto)).unwrap();
    let synrepo_dir = Config::synrepo_dir(repo.path());
    let mut overlay = SqliteOverlayStore::open(&synrepo_dir.join("overlay")).unwrap();

    let from = NodeId::Concept(ConceptNodeId(1));
    insert_n_candidates(&mut overlay, from, 75);

    let json = super::super::commands::links_list_output(repo.path(), None, Some(0), true).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(json.trim()).unwrap();
    let arr = parsed.as_array().expect("expected JSON array");
    assert_eq!(
        arr.len(),
        75,
        "--limit 0 must return every active candidate"
    );

    let text =
        super::super::commands::links_list_output(repo.path(), None, Some(0), false).unwrap();
    assert!(
        text.contains("Found 75 candidates."),
        "--limit 0 path must not surface the cap hint, got: {text}"
    );
}

/// Explicit non-zero `--limit` is honored verbatim and routed through
/// `candidates_limited` (SQL-side LIMIT).
#[test]
fn links_list_explicit_limit_honored() {
    let repo = tempdir().unwrap();
    bootstrap(repo.path(), Some(Mode::Auto)).unwrap();
    let synrepo_dir = Config::synrepo_dir(repo.path());
    let mut overlay = SqliteOverlayStore::open(&synrepo_dir.join("overlay")).unwrap();

    let from = NodeId::Concept(ConceptNodeId(1));
    insert_n_candidates(&mut overlay, from, 75);

    let json =
        super::super::commands::links_list_output(repo.path(), None, Some(10), true).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(json.trim()).unwrap();
    let arr = parsed.as_array().expect("expected JSON array");
    assert_eq!(arr.len(), 10, "explicit --limit must cap output");
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

/// Simulate a crash after Phase 1 (overlay marked pending) but before Phase 2
/// (graph edge not written). Replaying accept should detect pending_promotion,
/// see no graph edge, and proceed with the normal accept flow.
#[test]
fn links_accept_recovers_pending_promotion_without_edge() {
    let (repo, mut overlay, from, to) = setup_curated_link_env();
    overlay.insert_link(sample_link(from, to)).unwrap();

    // Simulate crash: manually set state to pending_promotion (Phase 1 done, Phase 2 not done).
    let synrepo_dir = Config::synrepo_dir(repo.path());
    let conn =
        rusqlite::Connection::open(SqliteOverlayStore::db_path(&synrepo_dir.join("overlay")))
            .unwrap();
    conn.execute(
        "UPDATE cross_links SET state = 'pending_promotion', reviewer = 'crash-test'",
        [],
    )
    .unwrap();
    drop(conn);

    let candidate_id = format_candidate_id(from, to, OverlayEdgeKind::References, "test-pass");
    super::super::commands::links_accept(repo.path(), &candidate_id, Some("reviewer-a")).unwrap();

    // Verify graph edge was written.
    let graph = SqliteGraphStore::open_existing(&synrepo_dir.join("graph")).unwrap();
    let edges = graph
        .outbound(from, Some(EdgeKind::References))
        .unwrap()
        .into_iter()
        .filter(|edge| edge.to == to)
        .collect::<Vec<_>>();
    assert_eq!(
        edges.len(),
        1,
        "accept must write graph edge after recovering from pending_promotion"
    );

    // Verify overlay state is promoted.
    let conn =
        rusqlite::Connection::open(SqliteOverlayStore::db_path(&synrepo_dir.join("overlay")))
            .unwrap();
    let state: String = conn
        .query_row(
            "SELECT state FROM cross_links WHERE from_node = ?1",
            [from.to_string()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(state, "promoted");
}

/// Simulate a crash after Phase 2 (graph edge written) but before Phase 3
/// (overlay not marked promoted). Replaying accept should detect
/// pending_promotion, find the graph edge, and complete Phase 3.
#[test]
fn links_accept_completes_pending_promotion_with_edge() {
    let (repo, mut overlay, from, to) = setup_curated_link_env();
    overlay.insert_link(sample_link(from, to)).unwrap();

    // Run accept once to write the graph edge, then manually regress overlay state.
    let candidate_id = format_candidate_id(from, to, OverlayEdgeKind::References, "test-pass");
    super::super::commands::links_accept(repo.path(), &candidate_id, Some("reviewer-a")).unwrap();

    let synrepo_dir = Config::synrepo_dir(repo.path());

    // Simulate crash: regress overlay state back to pending_promotion.
    let conn =
        rusqlite::Connection::open(SqliteOverlayStore::db_path(&synrepo_dir.join("overlay")))
            .unwrap();
    conn.execute("UPDATE cross_links SET state = 'pending_promotion'", [])
        .unwrap();
    drop(conn);

    // Replay accept: should detect pending_promotion, find the edge, complete Phase 3.
    super::super::commands::links_accept(repo.path(), &candidate_id, Some("reviewer-a")).unwrap();

    // Verify overlay state is promoted and edge count is still 1.
    let conn =
        rusqlite::Connection::open(SqliteOverlayStore::db_path(&synrepo_dir.join("overlay")))
            .unwrap();
    let state: String = conn
        .query_row(
            "SELECT state FROM cross_links WHERE from_node = ?1",
            [from.to_string()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(state, "promoted");

    let graph = SqliteGraphStore::open_existing(&synrepo_dir.join("graph")).unwrap();
    let edges = graph
        .outbound(from, Some(EdgeKind::References))
        .unwrap()
        .into_iter()
        .filter(|edge| edge.to == to)
        .collect::<Vec<_>>();
    assert_eq!(
        edges.len(),
        1,
        "replaying accept must not duplicate edges during crash recovery"
    );
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

// Open-for-create so `links_accept` malformed-input tests reach the parser
// branch instead of bailing on `open_existing`.
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
            .contains("links accept: watch service is active"),
        "expected watch-service guard error, got: {err}"
    );

    let reject_err =
        super::super::commands::links_reject(repo.path(), &candidate_id, None).unwrap_err();
    assert!(
        reject_err
            .to_string()
            .contains("links reject: watch service is active"),
        "expected watch-service guard error from reject, got: {reject_err}"
    );

    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(unix)]
#[test]
fn links_accept_fails_on_lock_conflict() {
    use synrepo::pipeline::writer::{
        hold_writer_flock_with_ownership, writer_lock_path, WriterOwnership,
    };

    let (repo, mut overlay, from, to) = setup_curated_link_env();
    overlay.insert_link(sample_link(from, to)).unwrap();

    let synrepo_dir = Config::synrepo_dir(repo.path());
    std::fs::create_dir_all(synrepo_dir.join("state")).unwrap();
    let mut holder = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .expect("spawn sleep");
    let ownership = WriterOwnership {
        pid: holder.id(),
        acquired_at: "2099-01-01T00:00:00Z".to_string(),
    };
    let _flock = hold_writer_flock_with_ownership(&writer_lock_path(&synrepo_dir), &ownership);

    let candidate_id = format_candidate_id(from, to, OverlayEdgeKind::References, "test-pass");
    let err = super::super::commands::links_accept(repo.path(), &candidate_id, Some("reviewer-a"))
        .unwrap_err();
    let _ = holder.kill();
    let _ = holder.wait();

    assert!(
        err.to_string().contains("writer lock held by pid"),
        "expected lock conflict error, got: {err}"
    );
}

#[cfg(unix)]
#[test]
fn links_reject_fails_on_lock_conflict() {
    use synrepo::pipeline::writer::{
        hold_writer_flock_with_ownership, writer_lock_path, WriterOwnership,
    };

    let (repo, mut overlay, from, to) = setup_curated_link_env();
    overlay.insert_link(sample_link(from, to)).unwrap();

    let synrepo_dir = Config::synrepo_dir(repo.path());
    std::fs::create_dir_all(synrepo_dir.join("state")).unwrap();
    let mut holder = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .expect("spawn sleep");
    let ownership = WriterOwnership {
        pid: holder.id(),
        acquired_at: "2099-01-01T00:00:00Z".to_string(),
    };
    let _flock = hold_writer_flock_with_ownership(&writer_lock_path(&synrepo_dir), &ownership);

    let candidate_id = format_candidate_id(from, to, OverlayEdgeKind::References, "test-pass");
    let err = super::super::commands::links_reject(repo.path(), &candidate_id, Some("reviewer-b"))
        .unwrap_err();
    let _ = holder.kill();
    let _ = holder.wait();

    assert!(
        err.to_string().contains("writer lock held by pid"),
        "expected lock conflict error, got: {err}"
    );
}

// `{phase}_fails_once` fires exactly once so the subsequent retry with real
// stores exercises recovery.
#[derive(Default)]
struct FailureSwitches {
    insert_edge_fails_once: bool,
    mark_promoted_fails_once: bool,
    delete_edge_fails_once: bool,
}

struct FailingStore<'a> {
    inner: RealLinksStore<'a>,
    switches: FailureSwitches,
}

impl LinksCommitStore for FailingStore<'_> {
    fn mark_pending(
        &mut self,
        from: NodeId,
        to: NodeId,
        kind: OverlayEdgeKind,
        reviewer: &str,
    ) -> anyhow::Result<()> {
        self.inner.mark_pending(from, to, kind, reviewer)
    }

    fn insert_edge(&mut self, edge: Edge) -> anyhow::Result<()> {
        if self.switches.insert_edge_fails_once {
            self.switches.insert_edge_fails_once = false;
            anyhow::bail!("injected: insert_edge failure");
        }
        self.inner.insert_edge(edge)
    }

    fn mark_promoted(
        &mut self,
        from: NodeId,
        to: NodeId,
        kind: OverlayEdgeKind,
        reviewer: &str,
        edge_id: &str,
    ) -> anyhow::Result<()> {
        if self.switches.mark_promoted_fails_once {
            self.switches.mark_promoted_fails_once = false;
            anyhow::bail!("injected: mark_promoted failure");
        }
        self.inner.mark_promoted(from, to, kind, reviewer, edge_id)
    }

    fn delete_edge(&mut self, edge_id: EdgeId) -> anyhow::Result<()> {
        if self.switches.delete_edge_fails_once {
            self.switches.delete_edge_fails_once = false;
            anyhow::bail!("injected: delete_edge (compensation) failure");
        }
        self.inner.delete_edge(edge_id)
    }
}

fn curated_edge_id(from: NodeId, to: NodeId, kind: EdgeKind) -> EdgeId {
    synrepo::pipeline::structural::derive_edge_id(from, to, kind)
}

// Direct SQL read: the trait surface does not expose intermediate states like
// `pending_promotion` that the fault-injection tests need to observe.
fn read_state(
    overlay_dir: &std::path::Path,
    from: NodeId,
    to: NodeId,
    kind: &str,
) -> Option<String> {
    let conn = rusqlite::Connection::open(SqliteOverlayStore::db_path(overlay_dir)).unwrap();
    conn.query_row(
        "SELECT state FROM cross_links WHERE from_node = ?1 AND to_node = ?2 AND kind = ?3",
        [from.to_string(), to.to_string(), kind.to_string()],
        |row| row.get::<_, String>(0),
    )
    .ok()
}

fn promoted_audit_count(overlay_dir: &std::path::Path, from: NodeId, to: NodeId) -> usize {
    let conn = rusqlite::Connection::open(SqliteOverlayStore::db_path(overlay_dir)).unwrap();
    conn.query_row(
        "SELECT COUNT(*) FROM cross_link_audit
         WHERE from_node = ?1 AND to_node = ?2 AND event_kind = 'promoted'",
        [from.to_string(), to.to_string()],
        |row| row.get::<_, i64>(0),
    )
    .unwrap() as usize
}

fn edge_exists(graph_dir: &std::path::Path, from: NodeId, to: NodeId) -> bool {
    let graph = SqliteGraphStore::open_existing(graph_dir).unwrap();
    graph
        .outbound(from, Some(EdgeKind::References))
        .unwrap()
        .iter()
        .any(|e| e.to == to)
}

#[test]
fn links_accept_commit_graph_insert_failure_leaves_pending_without_edge() {
    let (repo, mut overlay, from, to) = setup_curated_link_env();
    overlay.insert_link(sample_link(from, to)).unwrap();

    let synrepo_dir = Config::synrepo_dir(repo.path());
    let mut graph = SqliteGraphStore::open_existing(&synrepo_dir.join("graph")).unwrap();
    let edge_id = curated_edge_id(from, to, EdgeKind::References);

    {
        let mut store = FailingStore {
            inner: RealLinksStore {
                graph: &mut graph,
                overlay: &mut overlay,
            },
            switches: FailureSwitches {
                insert_edge_fails_once: true,
                ..Default::default()
            },
        };
        let err = super::super::commands::links_accept_commit(
            &mut store,
            &CommitArgs {
                from,
                to,
                kind: OverlayEdgeKind::References,
                edge_kind: EdgeKind::References,
                edge_id,
                reviewer: "reviewer-a",
            },
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("insert_edge failure"),
            "expected Phase 2 injection error, got: {err}"
        );
    }
    drop(graph);

    let overlay_dir = synrepo_dir.join("overlay");
    assert_eq!(
        read_state(&overlay_dir, from, to, "references").as_deref(),
        Some("pending_promotion"),
        "overlay must be pending_promotion after Phase 2 failure"
    );
    assert!(
        !edge_exists(&synrepo_dir.join("graph"), from, to),
        "graph edge must not exist after Phase 2 failure"
    );
    assert_eq!(
        promoted_audit_count(&overlay_dir, from, to),
        0,
        "no promoted audit row should exist yet"
    );

    drop(overlay);
    let candidate_id = format_candidate_id(from, to, OverlayEdgeKind::References, "test-pass");
    super::super::commands::links_accept(repo.path(), &candidate_id, Some("reviewer-a")).unwrap();

    assert!(edge_exists(&synrepo_dir.join("graph"), from, to));
    assert_eq!(
        read_state(&overlay_dir, from, to, "references").as_deref(),
        Some("promoted")
    );
    assert_eq!(promoted_audit_count(&overlay_dir, from, to), 1);
}

#[test]
fn links_accept_commit_overlay_finalize_failure_invokes_compensation() {
    let (repo, mut overlay, from, to) = setup_curated_link_env();
    overlay.insert_link(sample_link(from, to)).unwrap();

    let synrepo_dir = Config::synrepo_dir(repo.path());
    let mut graph = SqliteGraphStore::open_existing(&synrepo_dir.join("graph")).unwrap();
    let edge_id = curated_edge_id(from, to, EdgeKind::References);

    {
        let mut store = FailingStore {
            inner: RealLinksStore {
                graph: &mut graph,
                overlay: &mut overlay,
            },
            switches: FailureSwitches {
                mark_promoted_fails_once: true,
                ..Default::default()
            },
        };
        let err = super::super::commands::links_accept_commit(
            &mut store,
            &CommitArgs {
                from,
                to,
                kind: OverlayEdgeKind::References,
                edge_kind: EdgeKind::References,
                edge_id,
                reviewer: "reviewer-a",
            },
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("overlay finalize failed"),
            "expected overlay finalize error, got: {err}"
        );
        assert!(
            err.to_string().contains("mark_promoted failure"),
            "original overlay error must be preserved in message, got: {err}"
        );
    }
    drop(graph);
    drop(overlay);

    let overlay_dir = synrepo_dir.join("overlay");
    assert!(
        !edge_exists(&synrepo_dir.join("graph"), from, to),
        "compensation must have removed the graph edge"
    );
    assert_eq!(
        read_state(&overlay_dir, from, to, "references").as_deref(),
        Some("pending_promotion"),
        "overlay must still be pending_promotion (Phase 3 never completed)"
    );

    let candidate_id = format_candidate_id(from, to, OverlayEdgeKind::References, "test-pass");
    super::super::commands::links_accept(repo.path(), &candidate_id, Some("reviewer-a")).unwrap();

    let graph = SqliteGraphStore::open_existing(&synrepo_dir.join("graph")).unwrap();
    let edges: Vec<_> = graph
        .outbound(from, Some(EdgeKind::References))
        .unwrap()
        .into_iter()
        .filter(|e| e.to == to)
        .collect();
    assert_eq!(edges.len(), 1, "retry must not produce duplicate edges");
    assert_eq!(
        read_state(&overlay_dir, from, to, "references").as_deref(),
        Some("promoted")
    );
    assert_eq!(
        promoted_audit_count(&overlay_dir, from, to),
        1,
        "exactly one promotion audit row after rollback + retry"
    );
}

#[test]
fn links_accept_commit_both_failures_surface_original_error_and_inconsistency() {
    let (repo, mut overlay, from, to) = setup_curated_link_env();
    overlay.insert_link(sample_link(from, to)).unwrap();

    let synrepo_dir = Config::synrepo_dir(repo.path());
    let mut graph = SqliteGraphStore::open_existing(&synrepo_dir.join("graph")).unwrap();
    let edge_id = curated_edge_id(from, to, EdgeKind::References);

    {
        let mut store = FailingStore {
            inner: RealLinksStore {
                graph: &mut graph,
                overlay: &mut overlay,
            },
            switches: FailureSwitches {
                mark_promoted_fails_once: true,
                delete_edge_fails_once: true,
                ..Default::default()
            },
        };
        let err = super::super::commands::links_accept_commit(
            &mut store,
            &CommitArgs {
                from,
                to,
                kind: OverlayEdgeKind::References,
                edge_kind: EdgeKind::References,
                edge_id,
                reviewer: "reviewer-a",
            },
        )
        .unwrap_err();

        assert!(
            err.to_string().contains("overlay finalize failed"),
            "error must surface original overlay failure, got: {err}"
        );
        assert!(
            err.to_string().contains("mark_promoted failure"),
            "original overlay error text must be present, got: {err}"
        );
        assert!(
            !err.to_string().contains("delete_edge"),
            "compensation error must not mask original error, got: {err}"
        );
    }

    let overlay_dir = synrepo_dir.join("overlay");
    assert!(
        edge_exists(&synrepo_dir.join("graph"), from, to),
        "graph edge persists because compensation failed"
    );
    assert_eq!(
        read_state(&overlay_dir, from, to, "references").as_deref(),
        Some("pending_promotion"),
        "overlay state must NOT be promoted - this is the divergence signal"
    );
}

#[test]
fn links_accept_commit_rollback_then_retry_leaves_single_promoted_audit() {
    let (repo, mut overlay, from, to) = setup_curated_link_env();
    overlay.insert_link(sample_link(from, to)).unwrap();

    let synrepo_dir = Config::synrepo_dir(repo.path());
    let mut graph = SqliteGraphStore::open_existing(&synrepo_dir.join("graph")).unwrap();
    let edge_id = curated_edge_id(from, to, EdgeKind::References);

    {
        let mut store = FailingStore {
            inner: RealLinksStore {
                graph: &mut graph,
                overlay: &mut overlay,
            },
            switches: FailureSwitches {
                mark_promoted_fails_once: true,
                ..Default::default()
            },
        };
        let _err = super::super::commands::links_accept_commit(
            &mut store,
            &CommitArgs {
                from,
                to,
                kind: OverlayEdgeKind::References,
                edge_kind: EdgeKind::References,
                edge_id,
                reviewer: "reviewer-a",
            },
        )
        .unwrap_err();
    }
    drop(graph);
    drop(overlay);

    let overlay_dir = synrepo_dir.join("overlay");
    assert_eq!(
        promoted_audit_count(&overlay_dir, from, to),
        0,
        "no promoted audit row yet - Phase 3 never committed"
    );

    let candidate_id = format_candidate_id(from, to, OverlayEdgeKind::References, "test-pass");
    super::super::commands::links_accept(repo.path(), &candidate_id, Some("reviewer-a")).unwrap();

    assert_eq!(
        promoted_audit_count(&overlay_dir, from, to),
        1,
        "exactly one promotion audit row after retry"
    );
    assert_eq!(
        read_state(&overlay_dir, from, to, "references").as_deref(),
        Some("promoted")
    );

    // Third accept must be an idempotent no-op on the promoted-audit count.
    super::super::commands::links_accept(repo.path(), &candidate_id, Some("reviewer-a")).unwrap();
    assert_eq!(
        promoted_audit_count(&overlay_dir, from, to),
        1,
        "idempotent replay must not append a second promoted audit"
    );
}
