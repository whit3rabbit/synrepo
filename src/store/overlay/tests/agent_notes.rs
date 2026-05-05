//! Agent-note overlay tests.

use crate::{
    core::{
        ids::FileNodeId,
        provenance::{Provenance, SourceRef},
    },
    overlay::{
        AgentNote, AgentNoteConfidence, AgentNoteQuery, AgentNoteSourceHash, AgentNoteStatus,
        AgentNoteTarget, AgentNoteTargetKind, OverlayStore,
    },
    store::{overlay::SqliteOverlayStore, sqlite::SqliteGraphStore},
    structure::graph::{Epistemic, FileNode, GraphStore},
};
use tempfile::tempdir;

fn note(target: &str, claim: &str) -> AgentNote {
    AgentNote::new(
        AgentNoteTarget {
            kind: AgentNoteTargetKind::Path,
            id: target.to_string(),
        },
        claim.to_string(),
        "codex".to_string(),
        AgentNoteConfidence::Medium,
    )
}

fn file_node(path: &str, hash: &str) -> FileNode {
    FileNode {
        id: FileNodeId(42),
        root_id: "primary".to_string(),
        path: path.to_string(),
        path_history: Vec::new(),
        content_hash: hash.to_string(),
        content_sample_hashes: Vec::new(),
        size_bytes: 10,
        language: Some("rust".to_string()),
        inline_decisions: Vec::new(),
        last_observed_rev: Some(1),
        epistemic: Epistemic::ParserObserved,
        provenance: Provenance::structural(
            "test",
            "rev1",
            vec![SourceRef {
                file_id: Some(FileNodeId(42)),
                path: path.to_string(),
                content_hash: hash.to_string(),
            }],
        ),
    }
}

#[test]
fn note_without_evidence_is_unverified_and_advisory() {
    let dir = tempdir().unwrap();
    let mut store = SqliteOverlayStore::open(dir.path()).unwrap();

    let inserted = store.insert_note(note("src/lib.rs", "A claim")).unwrap();

    assert_eq!(inserted.status, AgentNoteStatus::Unverified);
    assert_eq!(inserted.source_store, "overlay");
    assert!(inserted.advisory);
    let counts = store.note_counts().unwrap();
    assert_eq!(counts.unverified, 1);
}

#[test]
fn forgotten_notes_are_hidden_from_normal_queries() {
    let dir = tempdir().unwrap();
    let mut store = SqliteOverlayStore::open(dir.path()).unwrap();
    let inserted = store.insert_note(note("src/lib.rs", "Forget me")).unwrap();

    store
        .forget_note(&inserted.note_id, "tester", Some("test tombstone"))
        .unwrap();

    let normal = store.query_notes(AgentNoteQuery::default()).unwrap();
    assert!(normal.is_empty());

    let audit = store
        .query_notes(AgentNoteQuery {
            include_forgotten: true,
            ..AgentNoteQuery::default()
        })
        .unwrap();
    assert_eq!(audit.len(), 1);
    assert_eq!(audit[0].status, AgentNoteStatus::Forgotten);
}

#[test]
fn supersede_preserves_old_note_and_links_replacement() {
    let dir = tempdir().unwrap();
    let mut store = SqliteOverlayStore::open(dir.path()).unwrap();
    let old = store.insert_note(note("src/lib.rs", "Old claim")).unwrap();
    let replacement = note("src/lib.rs", "New claim");

    let replacement = store
        .supersede_note(&old.note_id, replacement, "tester")
        .unwrap();

    let old = store.note_by_id(&old.note_id).unwrap().unwrap();
    assert_eq!(old.status, AgentNoteStatus::Superseded);
    assert_eq!(
        old.superseded_by.as_deref(),
        Some(replacement.note_id.as_str())
    );
    assert_eq!(replacement.supersedes, vec![old.note_id]);
}

#[test]
fn source_hash_drift_marks_note_stale() {
    let overlay_dir = tempdir().unwrap();
    let graph_dir = tempdir().unwrap();
    let mut overlay = SqliteOverlayStore::open(overlay_dir.path()).unwrap();
    let mut graph = SqliteGraphStore::open(graph_dir.path()).unwrap();
    graph
        .upsert_file(file_node("src/lib.rs", "hash-now"))
        .unwrap();

    let mut anchored = note("src/lib.rs", "Anchored claim");
    anchored.source_hashes.push(AgentNoteSourceHash {
        path: "src/lib.rs".to_string(),
        hash: "hash-then".to_string(),
        root_id: None,
    });
    let inserted = overlay.insert_note(anchored).unwrap();

    let stale = crate::store::overlay::current_drifted_note_ids(
        &overlay.all_notes().unwrap(),
        &graph,
        None,
    )
    .unwrap();
    assert_eq!(stale, vec![inserted.note_id.clone()]);

    overlay
        .mark_stale_notes(&stale, "test-drift-check")
        .unwrap();
    let updated = overlay.note_by_id(&inserted.note_id).unwrap().unwrap();
    assert_eq!(updated.status, AgentNoteStatus::Stale);
}
