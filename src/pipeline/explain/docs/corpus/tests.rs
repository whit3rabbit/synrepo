use super::*;
use std::str::FromStr;

use crate::overlay::CommentaryProvenance;
use crate::pipeline::explain::commentary_template::REQUIRED_SECTIONS;
use crate::surface::card::compiler::tests::fixtures::{fresh_symbol_fixture, make_overlay_store};
use time::OffsetDateTime;

fn structured_body() -> String {
    REQUIRED_SECTIONS
        .iter()
        .map(|section| format!("## {section}\nFixture note."))
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[test]
fn upsert_and_parse_commentary_doc_round_trip() {
    let repo = tempfile::tempdir().unwrap();
    let synrepo_dir = repo.path().join(".synrepo");
    let entry = CommentaryEntry {
        node_id: NodeId::from_str("sym_00000000000000000000000000000001").unwrap(),
        text: structured_body(),
        provenance: CommentaryProvenance {
            source_content_hash: "h1".to_string(),
            pass_id: "test".to_string(),
            model_identity: "fixture".to_string(),
            generated_at: OffsetDateTime::UNIX_EPOCH,
        },
    };
    let metadata = CommentaryDocSymbolMetadata {
        qualified_name: "crate::demo::run".to_string(),
        source_path: "src/lib.rs".to_string(),
    };

    let path = upsert_commentary_doc(
        &synrepo_dir,
        entry.node_id,
        &entry,
        FreshnessState::Fresh,
        &metadata,
    )
    .unwrap()
    .unwrap();
    let header = parse_commentary_doc_header(&path).unwrap().unwrap();
    assert_eq!(header.node_id, entry.node_id.to_string());
    assert_eq!(header.qualified_name, "crate::demo::run");
    assert_eq!(header.source_path, "src/lib.rs");
    assert_eq!(header.commentary_state, "fresh");
    assert_eq!(header.model_identity, "fixture");
    assert_eq!(header.node_kind, "symbol");
    assert_eq!(header.source_content_hash, "h1");
}

#[test]
fn reconcile_materializes_file_commentary_docs() {
    let (repo, graph, _sym_id) = fresh_symbol_fixture();
    let file = graph.file_by_path("src/lib.rs").unwrap().unwrap();
    let overlay = make_overlay_store(&repo);
    overlay
        .lock()
        .insert_commentary(CommentaryEntry {
            node_id: NodeId::File(file.id),
            text: format!("<think>hidden reasoning</think>\n{}", structured_body()),
            provenance: CommentaryProvenance {
                source_content_hash: file.content_hash.clone(),
                pass_id: "test".to_string(),
                model_identity: "fixture".to_string(),
                generated_at: OffsetDateTime::UNIX_EPOCH,
            },
        })
        .unwrap();

    let synrepo_dir = repo.path().join(".synrepo");
    let overlay_store = SqliteOverlayStore::open_existing(&synrepo_dir.join("overlay")).unwrap();
    let touched = reconcile_commentary_docs(&synrepo_dir, &graph, Some(&overlay_store)).unwrap();

    assert_eq!(touched.len(), 1);
    let doc_path = docs_root(&synrepo_dir)
        .join("files")
        .join(format!("{}.md", NodeId::File(file.id)));
    let header = parse_commentary_doc_header(&doc_path).unwrap().unwrap();
    assert_eq!(header.node_kind, "file");
    assert_eq!(header.source_path, "src/lib.rs");
    let rendered = std::fs::read_to_string(doc_path).unwrap();
    assert!(!rendered.contains("<think>"));
    for section in REQUIRED_SECTIONS {
        assert!(rendered.contains(&format!("## {section}")));
    }
}

#[test]
fn reconcile_removes_orphaned_commentary_docs() {
    let (repo, graph, sym_id) = fresh_symbol_fixture();
    let overlay = make_overlay_store(&repo);
    let entry = CommentaryEntry {
        node_id: NodeId::Symbol(sym_id),
        text: structured_body(),
        provenance: CommentaryProvenance {
            source_content_hash: graph
                .file_by_path("src/lib.rs")
                .unwrap()
                .unwrap()
                .content_hash,
            pass_id: "test".to_string(),
            model_identity: "fixture".to_string(),
            generated_at: OffsetDateTime::UNIX_EPOCH,
        },
    };
    overlay.lock().insert_commentary(entry.clone()).unwrap();

    let synrepo_dir = repo.path().join(".synrepo");
    let overlay_store = SqliteOverlayStore::open_existing(&synrepo_dir.join("overlay")).unwrap();
    let first = reconcile_commentary_docs(&synrepo_dir, &graph, Some(&overlay_store)).unwrap();
    assert_eq!(first.len(), 1);

    overlay.lock().prune_orphans(&[]).unwrap();
    let overlay_store = SqliteOverlayStore::open_existing(&synrepo_dir.join("overlay")).unwrap();
    let second = reconcile_commentary_docs(&synrepo_dir, &graph, Some(&overlay_store)).unwrap();
    assert_eq!(second.len(), 1);
    assert!(!docs_root(&synrepo_dir)
        .join("symbols")
        .join(format!("{}.md", NodeId::Symbol(sym_id)))
        .exists());
}

#[test]
fn reconcile_rewrites_commentary_state_when_overlay_entry_is_stale() {
    let (repo, graph, sym_id) = fresh_symbol_fixture();
    let overlay = make_overlay_store(&repo);
    overlay
        .lock()
        .insert_commentary(CommentaryEntry {
            node_id: NodeId::Symbol(sym_id),
            text: "Stale prose.".to_string(),
            provenance: CommentaryProvenance {
                source_content_hash: "outdated-hash".to_string(),
                pass_id: "test".to_string(),
                model_identity: "fixture".to_string(),
                generated_at: OffsetDateTime::UNIX_EPOCH,
            },
        })
        .unwrap();

    let synrepo_dir = repo.path().join(".synrepo");
    let overlay_store = SqliteOverlayStore::open_existing(&synrepo_dir.join("overlay")).unwrap();
    reconcile_commentary_docs(&synrepo_dir, &graph, Some(&overlay_store)).unwrap();

    let doc_path = docs_root(&synrepo_dir)
        .join("symbols")
        .join(format!("{}.md", NodeId::Symbol(sym_id)));
    let header = parse_commentary_doc_header(&doc_path).unwrap().unwrap();
    assert_eq!(header.commentary_state, "stale");
}
