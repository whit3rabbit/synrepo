use synrepo::core::ids::{FileNodeId, NodeId, SymbolNodeId};
use synrepo::store::overlay::SqliteOverlayStore;
use tempfile::tempdir;

use super::{insert_commentary_row, seed_graph, status_output};

#[test]
fn status_default_skips_freshness_scan() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());

    let synrepo_dir = synrepo::config::Config::synrepo_dir(repo.path());
    let overlay_dir = synrepo_dir.join("overlay");
    let mut overlay = SqliteOverlayStore::open(&overlay_dir).unwrap();
    insert_commentary_row(&mut overlay, NodeId::File(FileNodeId(0x42)), "abc123");
    insert_commentary_row(&mut overlay, NodeId::Symbol(SymbolNodeId(0x24)), "abc123");
    insert_commentary_row(
        &mut overlay,
        NodeId::Symbol(SymbolNodeId(0xdead01)),
        "stale",
    );
    insert_commentary_row(
        &mut overlay,
        NodeId::Symbol(SymbolNodeId(0xdead02)),
        "stale",
    );
    insert_commentary_row(
        &mut overlay,
        NodeId::Symbol(SymbolNodeId(0xdead03)),
        "stale",
    );
    drop(overlay);

    let text = status_output(repo.path(), false, false, false).unwrap();
    assert!(
        text.contains("commentary:   5 entries"),
        "expected cheap-path `5 entries`, got: {text}"
    );
    assert!(
        !text.contains("fresh /"),
        "default path must not render the `fresh / total` freshness summary, got: {text}"
    );

    let json: serde_json::Value = serde_json::from_str(
        status_output(repo.path(), true, false, false)
            .unwrap()
            .trim(),
    )
    .unwrap();
    assert_eq!(json["commentary_coverage"]["total"], 5);
    assert!(
        json["commentary_coverage"]["fresh"].is_null(),
        "cheap path must emit fresh: null, got: {}",
        json["commentary_coverage"]
    );
}

#[test]
fn status_full_computes_freshness() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());

    let synrepo_dir = synrepo::config::Config::synrepo_dir(repo.path());
    let overlay_dir = synrepo_dir.join("overlay");
    let mut overlay = SqliteOverlayStore::open(&overlay_dir).unwrap();
    insert_commentary_row(&mut overlay, NodeId::File(FileNodeId(0x42)), "abc123");
    insert_commentary_row(&mut overlay, NodeId::Symbol(SymbolNodeId(0x24)), "abc123");
    insert_commentary_row(
        &mut overlay,
        NodeId::Symbol(SymbolNodeId(0xdead01)),
        "abc123",
    );
    insert_commentary_row(
        &mut overlay,
        NodeId::Symbol(SymbolNodeId(0xdead02)),
        "abc123",
    );
    drop(overlay);

    let text = status_output(repo.path(), false, false, true).unwrap();
    assert!(
        text.contains("2 fresh / 4 total nodes with commentary"),
        "expected full-path freshness summary, got: {text}"
    );

    let json: serde_json::Value = serde_json::from_str(
        status_output(repo.path(), true, false, true)
            .unwrap()
            .trim(),
    )
    .unwrap();
    assert_eq!(json["commentary_coverage"]["total"], 4);
    assert_eq!(json["commentary_coverage"]["fresh"], 2);
}

#[test]
fn status_default_with_1000_commentary_rows_completes_quickly() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());

    let synrepo_dir = synrepo::config::Config::synrepo_dir(repo.path());
    let overlay_dir = synrepo_dir.join("overlay");
    let mut overlay = SqliteOverlayStore::open(&overlay_dir).unwrap();
    for i in 0..1000_u64 {
        insert_commentary_row(
            &mut overlay,
            NodeId::Symbol(SymbolNodeId((0x10_0000 + i) as u128)),
            "stale",
        );
    }
    drop(overlay);

    let start = std::time::Instant::now();
    let text = status_output(repo.path(), false, false, false).unwrap();
    let elapsed = start.elapsed();

    assert!(
        text.contains("commentary:   1000 entries"),
        "expected `1000 entries`, got: {text}"
    );
    assert!(
        elapsed < std::time::Duration::from_secs(1),
        "default status must stay cheap with 1000 commentary rows, took {elapsed:?}"
    );
}
