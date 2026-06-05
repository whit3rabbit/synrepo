use time::{format_description::well_known::Rfc3339, Duration, OffsetDateTime};

use crate::{
    bootstrap::bootstrap,
    config::Config,
    overlay::{
        AgentNote, AgentNoteConfidence, AgentNoteEvidence, AgentNoteSourceHash, AgentNoteTarget,
        AgentNoteTargetKind, OverlayStore,
    },
    store::{overlay::SqliteOverlayStore, sqlite::SqliteGraphStore},
    surface::lessons::{
        add_lesson, search_lessons, LessonAdd, LessonQuery, LESSON_EXPIRES_AT_KIND,
        LESSON_MARKER_ID, LESSON_MARKER_KIND,
    },
};

fn add_input(claim: &str) -> LessonAdd {
    LessonAdd {
        target_kind: AgentNoteTargetKind::Repo,
        target: ".".to_string(),
        claim: claim.to_string(),
        created_by: "test".to_string(),
        confidence: AgentNoteConfidence::Medium,
        evidence: vec![AgentNoteEvidence {
            kind: "text".to_string(),
            id: "evidence".to_string(),
        }],
        source_hashes: Vec::new(),
        graph_revision: None,
        ttl_seconds: Some(60),
    }
}

fn list_query(include_hidden: bool) -> LessonQuery {
    LessonQuery {
        query: None,
        target_kind: None,
        target: None,
        limit: 10,
        include_hidden,
    }
}

#[test]
fn lesson_marker_and_ttl_are_not_public_evidence() {
    let dir = tempfile::tempdir().unwrap();
    let mut overlay = SqliteOverlayStore::open(dir.path()).unwrap();

    let lesson = add_lesson(None, &mut overlay, add_input("Remember source rules")).unwrap();

    assert_eq!(lesson.evidence.len(), 1);
    assert_eq!(lesson.evidence[0].kind, "text");
    assert!(lesson.expires_at.is_some());

    let stored = overlay.note_by_id(&lesson.lesson_id).unwrap().unwrap();
    assert!(stored
        .evidence
        .iter()
        .any(|item| item.kind == LESSON_MARKER_KIND && item.id == LESSON_MARKER_ID));
    assert!(stored
        .evidence
        .iter()
        .any(|item| item.kind == LESSON_EXPIRES_AT_KIND));
}

#[test]
fn expired_lessons_are_hidden_unless_requested() {
    let dir = tempfile::tempdir().unwrap();
    let mut overlay = SqliteOverlayStore::open(dir.path()).unwrap();
    let mut note = AgentNote::new(
        AgentNoteTarget {
            kind: AgentNoteTargetKind::Repo,
            id: ".".to_string(),
        },
        "Expired lesson".to_string(),
        "test".to_string(),
        AgentNoteConfidence::Medium,
    );
    note.evidence.push(AgentNoteEvidence {
        kind: LESSON_MARKER_KIND.to_string(),
        id: LESSON_MARKER_ID.to_string(),
    });
    note.evidence.push(AgentNoteEvidence {
        kind: LESSON_EXPIRES_AT_KIND.to_string(),
        id: (OffsetDateTime::now_utc() - Duration::minutes(1))
            .format(&Rfc3339)
            .unwrap(),
    });
    overlay.insert_note(note).unwrap();

    let visible = search_lessons(None, &overlay, list_query(false)).unwrap();
    assert!(visible.is_empty());

    let hidden = search_lessons(None, &overlay, list_query(true)).unwrap();
    assert_eq!(hidden.len(), 1);
    assert_eq!(hidden[0].freshness, "expired");
}

#[test]
fn stale_source_hashes_are_labeled_without_mutating_graph() {
    let repo = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(repo.path().join("src")).unwrap();
    std::fs::write(
        repo.path().join("src/lib.rs"),
        "pub fn lesson_target() {}\n",
    )
    .unwrap();
    bootstrap(repo.path(), None, false).unwrap();
    let synrepo_dir = Config::synrepo_dir(repo.path());
    let mut overlay = SqliteOverlayStore::open(&synrepo_dir.join("overlay")).unwrap();
    let graph = SqliteGraphStore::open_existing(&synrepo_dir.join("graph")).unwrap();
    let before = graph.persisted_stats().unwrap();

    let mut input = add_input("Source anchors can drift");
    input.target_kind = AgentNoteTargetKind::Path;
    input.target = "src/lib.rs".to_string();
    input.source_hashes = vec![AgentNoteSourceHash {
        path: "src/lib.rs".to_string(),
        hash: "old-hash".to_string(),
        root_id: None,
    }];
    let lesson = add_lesson(Some(repo.path()), &mut overlay, input).unwrap();

    assert_eq!(lesson.freshness, "stale");
    let after = graph.persisted_stats().unwrap();
    assert_eq!(before.file_nodes, after.file_nodes);
    assert_eq!(before.symbol_nodes, after.symbol_nodes);
    assert_eq!(before.total_edges, after.total_edges);
}
