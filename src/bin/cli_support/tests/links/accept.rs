use synrepo::bootstrap::bootstrap;
use synrepo::config::{Config, Mode};
use synrepo::core::provenance::CreatedBy;
use synrepo::overlay::{OverlayEdgeKind, OverlayStore};
use synrepo::store::overlay::{format_candidate_id, SqliteOverlayStore};
use synrepo::store::sqlite::SqliteGraphStore;
use synrepo::structure::graph::{EdgeKind, Epistemic};
use tempfile::tempdir;

use super::{commands, sample_link, setup_curated_link_env};

fn serial_accept_guard() -> synrepo::test_support::GlobalTestLock {
    synrepo::test_support::global_test_lock("links-accept")
}

#[test]
fn links_accept_blocked_in_auto_mode() {
    let _guard = serial_accept_guard();
    let repo = tempdir().unwrap();
    bootstrap(repo.path(), Some(Mode::Auto)).unwrap();

    let err = commands::links_accept(
        repo.path(),
        "concept_0000000000000001::sym_0000000000000002::references",
        None,
    )
    .unwrap_err();
    assert!(err.to_string().contains("only available in `curated` mode"));
}

#[test]
fn links_accept_writes_human_declared_edge() {
    let _guard = serial_accept_guard();
    let (repo, mut overlay, from, to) = setup_curated_link_env();
    overlay.insert_link(sample_link(from, to)).unwrap();

    let candidate_id = format_candidate_id(from, to, OverlayEdgeKind::References, "test-pass");
    commands::links_accept(repo.path(), &candidate_id, Some("reviewer-a")).unwrap();

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
    let _guard = serial_accept_guard();
    // Two accepts must produce exactly one graph edge: the compensation path
    // depends on `insert_edge` being idempotent for crash-recovery reruns.
    let (repo, mut overlay, from, to) = setup_curated_link_env();
    overlay.insert_link(sample_link(from, to)).unwrap();

    let candidate_id = format_candidate_id(from, to, OverlayEdgeKind::References, "test-pass");
    commands::links_accept(repo.path(), &candidate_id, Some("reviewer-a")).unwrap();
    commands::links_accept(repo.path(), &candidate_id, Some("reviewer-a")).unwrap();

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
    let _guard = serial_accept_guard();
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
    commands::links_accept(repo.path(), &candidate_id, Some("reviewer-a")).unwrap();

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
    let _guard = serial_accept_guard();
    let (repo, mut overlay, from, to) = setup_curated_link_env();
    overlay.insert_link(sample_link(from, to)).unwrap();

    // Run accept once to write the graph edge, then manually regress overlay state.
    let candidate_id = format_candidate_id(from, to, OverlayEdgeKind::References, "test-pass");
    commands::links_accept(repo.path(), &candidate_id, Some("reviewer-a")).unwrap();

    let synrepo_dir = Config::synrepo_dir(repo.path());

    // Simulate crash: regress overlay state back to pending_promotion.
    let conn =
        rusqlite::Connection::open(SqliteOverlayStore::db_path(&synrepo_dir.join("overlay")))
            .unwrap();
    conn.execute("UPDATE cross_links SET state = 'pending_promotion'", [])
        .unwrap();
    drop(conn);

    // Replay accept: should detect pending_promotion, find the edge, complete Phase 3.
    commands::links_accept(repo.path(), &candidate_id, Some("reviewer-a")).unwrap();

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
    let _guard = serial_accept_guard();
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

    let err = commands::links_accept(repo.path(), &stale_id, Some("reviewer-a")).unwrap_err();
    assert!(
        err.to_string().contains("Stale review"),
        "expected stale-review diagnostic, got: {err}"
    );
}

#[test]
fn links_accept_current_revision_succeeds() {
    let _guard = serial_accept_guard();
    let (repo, mut overlay, from, to) = setup_curated_link_env();

    let mut link = sample_link(from, to);
    link.provenance.pass_id = "fresh-pass-abcdef".into();
    overlay.insert_link(link).unwrap();

    let id = format_candidate_id(from, to, OverlayEdgeKind::References, "fresh-pass-abcdef");
    commands::links_accept(repo.path(), &id, Some("reviewer-a")).unwrap();
}
