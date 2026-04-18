use synrepo::config::Config;
use synrepo::core::ids::{NodeId, SymbolNodeId};
use synrepo::overlay::{ConfidenceTier, OverlayStore};
use synrepo::store::overlay::SqliteOverlayStore;
use tempfile::tempdir;

use super::support::seed_graph;
use super::{commands, sample_link, write_curated_mode};

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
        let mut link = sample_link(from, NodeId::Symbol(SymbolNodeId(idx as u128)));
        link.confidence_score = score;
        link.confidence_tier = ConfidenceTier::ReviewQueue;
        overlay.insert_link(link).unwrap();
    }

    let json_text = commands::links_review_output(repo.path(), Some(2), true).unwrap();
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
