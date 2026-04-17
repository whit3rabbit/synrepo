use synrepo::bootstrap::bootstrap;
use synrepo::config::{Config, Mode};
use synrepo::core::ids::{ConceptNodeId, NodeId, SymbolNodeId};
use synrepo::overlay::OverlayStore;
use synrepo::store::overlay::{compare_score_desc, SqliteOverlayStore};
use tempfile::tempdir;

use super::{commands, sample_link};

fn insert_n_candidates(overlay: &mut SqliteOverlayStore, from: NodeId, n: u64) {
    for i in 0..n {
        let to = NodeId::Symbol(SymbolNodeId(1000 + i));
        overlay.insert_link(sample_link(from, to)).unwrap();
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
    let json_text = commands::links_list_output(repo.path(), None, None, true).unwrap();
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
    let text = commands::links_list_output(repo.path(), None, None, false).unwrap();
    assert!(
        text.contains("Found 1 candidates."),
        "expected count line, got: {text}"
    );
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

    let json = commands::links_list_output(repo.path(), None, None, true).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(json.trim()).unwrap();
    let arr = parsed.as_array().expect("expected JSON array");
    assert_eq!(arr.len(), 50, "default limit must cap JSON output at 50");

    let text = commands::links_list_output(repo.path(), None, None, false).unwrap();
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

    let json = commands::links_list_output(repo.path(), None, Some(0), true).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(json.trim()).unwrap();
    let arr = parsed.as_array().expect("expected JSON array");
    assert_eq!(
        arr.len(),
        75,
        "--limit 0 must return every active candidate"
    );

    let text = commands::links_list_output(repo.path(), None, Some(0), false).unwrap();
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

    let json = commands::links_list_output(repo.path(), None, Some(10), true).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(json.trim()).unwrap();
    let arr = parsed.as_array().expect("expected JSON array");
    assert_eq!(arr.len(), 10, "explicit --limit must cap output");
}

#[test]
fn compare_score_desc_tolerates_nan() {
    let mut scores = [f32::NAN, 0.5, 0.9];
    scores.sort_by(|a, b| compare_score_desc(*a, *b));
    let finite: Vec<f32> = scores.iter().copied().filter(|s| !s.is_nan()).collect();
    assert_eq!(finite, vec![0.9, 0.5]);
}
