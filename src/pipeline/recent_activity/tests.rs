use super::*;
use crate::pipeline::repair::{append_resolution_log, ResolutionLogEntry, SyncOutcome};
use tempfile::tempdir;

// 5.1: read_repair_events returns entries in reverse-chronological order and respects limit
#[test]
fn repair_events_reverse_chronological_and_limited() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");

    for i in 1..=3u32 {
        let entry = ResolutionLogEntry {
            synced_at: format!("2026-01-0{i}T00:00:00Z"),
            source_revision: None,
            requested_scope: vec![],
            findings_considered: vec![],
            actions_taken: vec![format!("action {i}")],
            outcome: SyncOutcome::Completed,
        };
        append_resolution_log(&synrepo_dir, &entry);
    }

    // limit 2 → entries 3, then 2 (most recent first)
    let events = read_repair_events(&synrepo_dir, 2, None);
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].timestamp, "2026-01-03T00:00:00Z");
    assert_eq!(events[1].timestamp, "2026-01-02T00:00:00Z");
    assert!(events.iter().all(|e| e.kind == "repair"));
}

// 5.2: read_reconcile_event returns None when no file exists, Some with note when it does
#[test]
fn reconcile_event_none_when_missing_some_when_present() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");

    // No state file yet → None.
    assert!(read_reconcile_event(&synrepo_dir).is_none());

    // Write minimal reconcile-state.json manually.
    let state_dir = synrepo_dir.join("state");
    std::fs::create_dir_all(&state_dir).unwrap();
    let state_json = r#"{
        "last_reconcile_at": "2026-01-01T00:00:00Z",
        "last_outcome": "completed",
        "last_error": null,
        "triggering_events": 0,
        "files_discovered": 10,
        "symbols_extracted": 50
    }"#;
    std::fs::write(state_dir.join("reconcile-state.json"), state_json).unwrap();

    let entry = read_reconcile_event(&synrepo_dir).unwrap();
    assert_eq!(entry.kind, "reconcile");
    assert_eq!(entry.timestamp, "2026-01-01T00:00:00Z");
    assert_eq!(entry.payload["note"], "single_entry");
    assert_eq!(entry.payload["outcome"], "completed");
}

// 5.3: read_recent_activity with limit > 200 returns an error
#[test]
fn recent_activity_limit_exceeded_returns_error() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");
    let config = crate::config::Config::default();
    let query = RecentActivityQuery {
        kinds: None,
        limit: 201,
        since: None,
    };
    let result = read_recent_activity(&synrepo_dir, dir.path(), &config, query);
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("201"),
        "error should mention requested limit: {msg}"
    );
    assert!(msg.contains("200"), "error should mention max: {msg}");
}

// 5.4: read_recent_activity with no git repo returns unavailable hotspot entry
#[test]
fn recent_activity_no_git_returns_unavailable_hotspot() {
    let dir = tempdir().unwrap();
    // dir has no .git/ → git is absent.
    let config = crate::config::Config::default();
    let events = read_hotspot_events(dir.path(), &config, 5);
    assert_eq!(events.len(), 1, "expected exactly one unavailable entry");
    assert_eq!(events[0].payload["state"], "unavailable");
    assert_eq!(events[0].kind, "hotspot");
}

// 5.5: kinds filter includes only the requested event kinds
#[test]
fn kinds_filter_restricts_to_requested_kinds() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");

    // Write one repair event.
    let entry = ResolutionLogEntry {
        synced_at: "2026-01-01T00:00:00Z".to_string(),
        source_revision: None,
        requested_scope: vec![],
        findings_considered: vec![],
        actions_taken: vec!["action 1".to_string()],
        outcome: SyncOutcome::Completed,
    };
    append_resolution_log(&synrepo_dir, &entry);

    let config = crate::config::Config::default();
    let query = RecentActivityQuery {
        kinds: Some(vec![RecentActivityKind::Repair]),
        limit: 10,
        since: None,
    };
    let results = read_recent_activity(&synrepo_dir, dir.path(), &config, query).unwrap();
    assert!(!results.is_empty(), "expected at least one repair event");
    for event in &results {
        assert_eq!(event.kind, "repair", "unexpected kind: {}", event.kind);
    }

    // Requesting only Reconcile with no state file → empty.
    let query2 = RecentActivityQuery {
        kinds: Some(vec![RecentActivityKind::Reconcile]),
        limit: 10,
        since: None,
    };
    let results2 = read_recent_activity(&synrepo_dir, dir.path(), &config, query2).unwrap();
    assert!(
        results2.is_empty(),
        "no reconcile state file should yield empty results"
    );
}
