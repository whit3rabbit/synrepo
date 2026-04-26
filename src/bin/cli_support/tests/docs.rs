use std::fs;

use synrepo::config::Config;
use synrepo::overlay::{CommentaryEntry, CommentaryProvenance, OverlayStore};
use synrepo::store::overlay::SqliteOverlayStore;
use synrepo::NodeId;
use tempfile::tempdir;
use time::OffsetDateTime;

use super::super::commands::{
    docs_export_output, docs_import_output, docs_list_output, docs_search_output,
};
use super::support::seed_graph;

#[test]
fn docs_export_materializes_file_commentary() {
    let repo = tempdir().unwrap();
    let ids = seed_graph(repo.path());
    seed_file_commentary(repo.path(), NodeId::File(ids.file_id), "File prose.");

    let output = docs_export_output(repo.path()).unwrap();

    assert!(output.contains("1 docs"), "unexpected output: {output}");
    assert!(doc_path(repo.path(), NodeId::File(ids.file_id)).exists());
}

#[test]
fn docs_list_and_search_read_materialized_docs() {
    let repo = tempdir().unwrap();
    let ids = seed_graph(repo.path());
    seed_file_commentary(repo.path(), NodeId::File(ids.file_id), "Needle prose.");
    docs_export_output(repo.path()).unwrap();

    let list = docs_list_output(repo.path()).unwrap();
    let search = docs_search_output(repo.path(), "Needle", 10).unwrap();

    assert!(list.contains("files"), "unexpected list output: {list}");
    assert!(
        list.contains("src/lib.rs"),
        "unexpected list output: {list}"
    );
    assert!(
        search.contains("Needle prose."),
        "unexpected search output: {search}"
    );
}

#[test]
fn docs_import_persists_edited_body() {
    let repo = tempdir().unwrap();
    let ids = seed_graph(repo.path());
    let node_id = NodeId::File(ids.file_id);
    seed_file_commentary(repo.path(), node_id, "File prose.");
    docs_export_output(repo.path()).unwrap();
    let path = doc_path(repo.path(), node_id);
    let text = fs::read_to_string(&path).unwrap();
    fs::write(&path, text.replace("File prose.", "Reviewed prose.")).unwrap();

    let output = docs_import_output(repo.path(), false, Some(&path)).unwrap();

    assert!(output.contains("1 imported"), "unexpected output: {output}");
    let overlay =
        SqliteOverlayStore::open_existing(&Config::synrepo_dir(repo.path()).join("overlay"))
            .unwrap();
    let entry = overlay.commentary_for(node_id).unwrap().unwrap();
    assert_eq!(entry.text, "Reviewed prose.");
    assert_eq!(entry.provenance.model_identity, "user-edited");
}

#[test]
fn docs_import_skips_stale_source_hash() {
    let repo = tempdir().unwrap();
    let ids = seed_graph(repo.path());
    let node_id = NodeId::File(ids.file_id);
    seed_file_commentary(repo.path(), node_id, "File prose.");
    docs_export_output(repo.path()).unwrap();
    let path = doc_path(repo.path(), node_id);
    let text = fs::read_to_string(&path).unwrap();
    fs::write(
        &path,
        text.replace("source_content_hash: abc123", "source_content_hash: stale"),
    )
    .unwrap();

    let output = docs_import_output(repo.path(), true, None).unwrap();

    assert!(output.contains("0 imported"), "unexpected output: {output}");
    assert!(output.contains("1 skipped"), "unexpected output: {output}");
}

fn seed_file_commentary(repo: &std::path::Path, node_id: NodeId, text: &str) {
    let mut overlay = SqliteOverlayStore::open(&Config::synrepo_dir(repo).join("overlay")).unwrap();
    overlay
        .insert_commentary(CommentaryEntry {
            node_id,
            text: text.to_string(),
            provenance: CommentaryProvenance {
                source_content_hash: "abc123".to_string(),
                pass_id: "test".to_string(),
                model_identity: "fixture".to_string(),
                generated_at: OffsetDateTime::UNIX_EPOCH,
            },
        })
        .unwrap();
}

fn doc_path(repo: &std::path::Path, node_id: NodeId) -> std::path::PathBuf {
    Config::synrepo_dir(repo)
        .join("explain-docs")
        .join("files")
        .join(format!("{node_id}.md"))
}
