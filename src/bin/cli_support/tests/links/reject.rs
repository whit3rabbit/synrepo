use synrepo::bootstrap::bootstrap;
use synrepo::config::{Config, Mode};
use synrepo::core::ids::NodeId;
use synrepo::overlay::{OverlayEdgeKind, OverlayStore};
use synrepo::store::overlay::{format_candidate_id, SqliteOverlayStore};
use tempfile::tempdir;

use super::support::seed_graph;
use super::{commands, sample_link, write_curated_mode};

#[test]
fn links_reject_blocked_in_auto_mode() {
    let repo = tempdir().unwrap();
    bootstrap(repo.path(), Some(Mode::Auto), false).unwrap();

    let err = commands::links_reject(
        repo.path(),
        "concept_0000000000000001::sym_0000000000000002::references",
        None,
    )
    .unwrap_err();
    assert!(err.to_string().contains("only available in `curated` mode"));
}

#[test]
fn links_reject_updates_candidate_state() {
    let repo = tempdir().unwrap();
    let ids = seed_graph(repo.path());
    write_curated_mode(repo.path());

    let synrepo_dir = Config::synrepo_dir(repo.path());
    let mut overlay = SqliteOverlayStore::open(&synrepo_dir.join("overlay")).unwrap();
    let from = NodeId::Concept(ids.concept_id);
    let to = NodeId::Symbol(ids.symbol_id);
    overlay.insert_link(sample_link(from, to)).unwrap();

    let candidate_id = format_candidate_id(from, to, OverlayEdgeKind::References, "test-pass");
    commands::links_reject(repo.path(), &candidate_id, Some("reviewer-b")).unwrap();

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
