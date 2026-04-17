use synrepo::bootstrap::bootstrap;
use synrepo::config::{Config, Mode};
use synrepo::core::ids::{ConceptNodeId, NodeId, SymbolNodeId};
use synrepo::overlay::{ConfidenceTier, OverlayStore};
use synrepo::store::overlay::{FindingsFilter, SqliteOverlayStore};
use synrepo::store::sqlite::SqliteGraphStore;
use tempfile::tempdir;

use super::support::seed_graph;
use super::{commands, sample_link};

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
    let text = commands::findings_output(repo.path(), None, None, None, Some(1), false).unwrap();
    assert!(
        text.contains("Found 1 findings."),
        "expected `Found 1 findings.` line (limit honored), got: {text}"
    );

    // Without a limit, both candidates appear. This locks down that the
    // limit value flows from CLI option to overlay query.
    let unlimited = commands::findings_output(repo.path(), None, None, None, None, false).unwrap();
    assert!(
        unlimited.contains("Found 2 findings."),
        "expected `Found 2 findings.` line without limit, got: {unlimited}"
    );
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

    commands::findings(repo.path(), None, None, None, Some(10), false).unwrap();

    let findings = overlay
        .findings(&graph, &FindingsFilter::default())
        .unwrap();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].tier, ConfidenceTier::BelowThreshold);
}
