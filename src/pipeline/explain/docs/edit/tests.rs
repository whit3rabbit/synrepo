use std::fs;

use time::OffsetDateTime;

use super::*;
use crate::overlay::{CommentaryEntry, CommentaryProvenance, FreshnessState, OverlayStore};
use crate::pipeline::explain::docs::{
    docs_root, upsert_commentary_doc, CommentaryDocSymbolMetadata,
};
use crate::surface::card::compiler::tests::fixtures::fresh_symbol_fixture;
use crate::{store::overlay::SqliteOverlayStore, NodeId};

#[test]
fn import_commentary_doc_updates_overlay_from_editable_body() {
    let (repo, graph, _sym_id) = fresh_symbol_fixture();
    let synrepo_dir = repo.path().join(".synrepo");
    let file = graph.file_by_path("src/lib.rs").unwrap().unwrap();
    let node_id = NodeId::File(file.id);
    let doc_path = write_file_doc(&synrepo_dir, node_id, &file.path, &file.content_hash);
    let text = fs::read_to_string(&doc_path).unwrap();
    fs::write(&doc_path, text.replace("Fresh prose.", "Edited prose.")).unwrap();

    let mut overlay = SqliteOverlayStore::open(&synrepo_dir.join("overlay")).unwrap();
    let outcome = import_commentary_doc(&graph, &mut overlay, &doc_path).unwrap();

    assert_eq!(outcome.status, CommentaryDocImportStatus::Imported);
    let entry = overlay.commentary_for(node_id).unwrap().unwrap();
    assert_eq!(entry.text, "Edited prose.");
    assert_eq!(entry.provenance.model_identity, "user-edited");
}

#[test]
fn import_commentary_doc_skips_when_source_hash_is_stale() {
    let (repo, graph, _sym_id) = fresh_symbol_fixture();
    let synrepo_dir = repo.path().join(".synrepo");
    let file = graph.file_by_path("src/lib.rs").unwrap().unwrap();
    let node_id = NodeId::File(file.id);
    let doc_path = write_file_doc(&synrepo_dir, node_id, &file.path, "outdated");

    let mut overlay = SqliteOverlayStore::open(&synrepo_dir.join("overlay")).unwrap();
    let outcome = import_commentary_doc(&graph, &mut overlay, &doc_path).unwrap();

    assert_eq!(outcome.status, CommentaryDocImportStatus::Skipped);
    assert_eq!(
        outcome.reason.as_deref(),
        Some("source_content_hash does not match current graph")
    );
    assert!(overlay.commentary_for(node_id).unwrap().is_none());
}

#[test]
fn list_commentary_docs_returns_file_docs() {
    let (repo, graph, _sym_id) = fresh_symbol_fixture();
    let synrepo_dir = repo.path().join(".synrepo");
    let file = graph.file_by_path("src/lib.rs").unwrap().unwrap();
    let node_id = NodeId::File(file.id);
    write_file_doc(&synrepo_dir, node_id, &file.path, &file.content_hash);

    let docs = list_commentary_docs(&synrepo_dir).unwrap();

    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0].node_id, node_id.to_string());
    assert_eq!(docs[0].node_kind, "file");
}

#[test]
fn commentary_doc_paths_only_returns_file_and_symbol_docs() {
    let (repo, graph, _sym_id) = fresh_symbol_fixture();
    let synrepo_dir = repo.path().join(".synrepo");
    let file = graph.file_by_path("src/lib.rs").unwrap().unwrap();
    let node_id = NodeId::File(file.id);
    let doc_path = write_file_doc(&synrepo_dir, node_id, &file.path, &file.content_hash);
    fs::write(docs_root(&synrepo_dir).join("index.md"), "# Support\n").unwrap();
    fs::write(docs_root(&synrepo_dir).join("llms.txt"), "# Support\n").unwrap();

    let paths = commentary_doc_paths(&synrepo_dir).unwrap();

    assert_eq!(paths, vec![doc_path]);
}

fn write_file_doc(
    synrepo_dir: &std::path::Path,
    node_id: NodeId,
    source_path: &str,
    source_hash: &str,
) -> std::path::PathBuf {
    let entry = CommentaryEntry {
        node_id,
        text: "Fresh prose.".to_string(),
        provenance: CommentaryProvenance {
            source_content_hash: source_hash.to_string(),
            pass_id: "test".to_string(),
            model_identity: "fixture".to_string(),
            generated_at: OffsetDateTime::UNIX_EPOCH,
        },
    };
    upsert_commentary_doc(
        synrepo_dir,
        node_id,
        &entry,
        FreshnessState::Fresh,
        &CommentaryDocSymbolMetadata {
            qualified_name: String::new(),
            source_path: source_path.to_string(),
        },
    )
    .unwrap()
    .unwrap_or_else(|| {
        docs_root(synrepo_dir)
            .join("files")
            .join(format!("{node_id}.md"))
    })
}
