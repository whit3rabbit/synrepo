use tempfile::tempdir;

use crate::pipeline::repair::{
    append_resolution_log, repair_log_path, RepairSurface, ResolutionLogEntry, SyncOutcome,
};

#[test]
fn resolution_log_appends_jsonl_entries() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");
    std::fs::create_dir_all(synrepo_dir.join("state")).unwrap();

    let entry1 = ResolutionLogEntry {
        synced_at: "2026-01-01T00:00:00Z".to_string(),
        source_revision: None,
        requested_scope: vec![RepairSurface::StoreMaintenance],
        findings_considered: vec![],
        actions_taken: vec!["ran maintenance".to_string()],
        outcome: SyncOutcome::Completed,
    };
    let entry2 = ResolutionLogEntry {
        synced_at: "2026-01-01T01:00:00Z".to_string(),
        source_revision: Some("abc".to_string()),
        requested_scope: vec![RepairSurface::StructuralRefresh],
        findings_considered: vec![],
        actions_taken: vec!["ran reconcile".to_string()],
        outcome: SyncOutcome::Completed,
    };

    append_resolution_log(&synrepo_dir, &entry1);
    append_resolution_log(&synrepo_dir, &entry2);

    let log_content = std::fs::read_to_string(repair_log_path(&synrepo_dir)).unwrap();
    let lines: Vec<&str> = log_content.lines().collect();

    assert_eq!(lines.len(), 2, "two entries must produce two JSONL lines");
    let decoded1: ResolutionLogEntry = serde_json::from_str(lines[0]).unwrap();
    let decoded2: ResolutionLogEntry = serde_json::from_str(lines[1]).unwrap();
    assert_eq!(decoded1.outcome, SyncOutcome::Completed);
    assert_eq!(decoded2.source_revision, Some("abc".to_string()));
}

#[test]
fn resolution_log_append_is_additive() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");

    let entry = ResolutionLogEntry {
        synced_at: "2026-01-01T00:00:00Z".to_string(),
        source_revision: None,
        requested_scope: vec![],
        findings_considered: vec![],
        actions_taken: vec![],
        outcome: SyncOutcome::Completed,
    };

    for _ in 0..3 {
        append_resolution_log(&synrepo_dir, &entry);
    }

    let content = std::fs::read_to_string(repair_log_path(&synrepo_dir)).unwrap();
    assert_eq!(content.lines().count(), 3);
}
