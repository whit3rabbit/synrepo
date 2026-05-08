use std::fs;

use time::OffsetDateTime;

use super::*;
use crate::overlay::{CommentaryEntry, CommentaryProvenance};
use crate::pipeline::explain::docs::reconcile_commentary_docs;
use crate::store::overlay::SqliteOverlayStore;
use crate::surface::card::compiler::tests::fixtures::{fresh_symbol_fixture, make_overlay_store};

#[test]
fn byte_ranges_map_to_one_based_lines() {
    let source = b"first\nsecond\nthird\n";
    assert_eq!(byte_range_to_line_range(source, (0, 5)), (1, 1));
    assert_eq!(byte_range_to_line_range(source, (6, 12)), (2, 2));
    assert_eq!(byte_range_to_line_range(source, (6, 18)), (2, 3));
}

#[test]
fn discovery_artifacts_include_symbol_metadata_and_line_range() {
    let (repo, graph, sym_id) = fresh_symbol_fixture();
    let file = graph.file_by_path("src/lib.rs").unwrap().unwrap();
    let overlay = make_overlay_store(&repo);
    overlay
        .lock()
        .insert_commentary(CommentaryEntry {
            node_id: NodeId::Symbol(sym_id),
            text: "Symbol prose.".to_string(),
            provenance: CommentaryProvenance {
                source_content_hash: file.content_hash,
                pass_id: "test".to_string(),
                model_identity: "fixture".to_string(),
                generated_at: OffsetDateTime::UNIX_EPOCH,
            },
        })
        .unwrap();

    let synrepo_dir = repo.path().join(".synrepo");
    let overlay_store = SqliteOverlayStore::open_existing(&synrepo_dir.join("overlay")).unwrap();
    reconcile_commentary_docs(&synrepo_dir, &graph, Some(&overlay_store)).unwrap();

    let summary = write_discovery_artifacts(&synrepo_dir, &graph).unwrap();
    let catalogue_path = docs_root(&synrepo_dir).join(CATALOGUE_JSON);
    let catalogue: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(catalogue_path).unwrap()).unwrap();
    let doc = &catalogue["docs"][0];

    assert_eq!(summary.total_artifacts, 3);
    assert_eq!(catalogue["source_store"], "overlay");
    assert_eq!(doc["node_id"], NodeId::Symbol(sym_id).to_string());
    assert_eq!(doc["node_kind"], "symbol");
    assert_eq!(doc["source_path"], "src/lib.rs");
    assert_eq!(doc["source_store"], "overlay");
    assert_eq!(doc["commentary_state"], "fresh");
    assert_eq!(doc["model_identity"], "fixture");
    assert!(doc["doc_path"].as_str().unwrap().starts_with("symbols/"));
    assert!(doc["source_reference"]["line_start"].as_u64().unwrap() >= 1);
    assert!(docs_root(&synrepo_dir).join(INDEX_MD).exists());
    assert!(docs_root(&synrepo_dir).join(LLMS_TXT).exists());
}
