use synrepo::config::Config;
use synrepo::pipeline::repair::{
    append_resolution_log, repair_log_degraded_marker_path, ResolutionLogEntry, SyncOutcome,
};
use tempfile::tempdir;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use super::{seed_graph, status_output};

#[test]
fn status_reports_repair_audit_degraded_when_marker_present() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());
    let synrepo_dir = Config::synrepo_dir(repo.path());

    let json_before: serde_json::Value = serde_json::from_str(
        status_output(repo.path(), true, false, false)
            .unwrap()
            .trim(),
    )
    .unwrap();
    assert_eq!(
        json_before["repair_audit"]["status"], "ok",
        "baseline must report ok, got: {json_before}"
    );

    let text_before = status_output(repo.path(), false, false, false).unwrap();
    assert!(
        text_before.contains("repair audit: ok"),
        "baseline human output must report ok, got: {text_before}"
    );
    assert!(
        !text_before.contains("unavailable"),
        "baseline must not mention unavailable, got: {text_before}"
    );

    let marker_path = repair_log_degraded_marker_path(&synrepo_dir);
    std::fs::create_dir_all(marker_path.parent().unwrap()).unwrap();
    std::fs::write(
        &marker_path,
        r#"{"last_failure_at":"2099-01-01T00:00:00Z","last_failure_reason":"open failed: test injection"}"#,
    )
    .unwrap();

    let json_after: serde_json::Value = serde_json::from_str(
        status_output(repo.path(), true, false, false)
            .unwrap()
            .trim(),
    )
    .unwrap();
    assert_eq!(
        json_after["repair_audit"]["status"], "unavailable",
        "marker must flip status to unavailable, got: {json_after}"
    );
    assert_eq!(
        json_after["repair_audit"]["last_failure_at"], "2099-01-01T00:00:00Z",
        "last_failure_at must be surfaced verbatim, got: {json_after}"
    );
    assert_eq!(
        json_after["repair_audit"]["last_failure_reason"], "open failed: test injection",
        "last_failure_reason must be surfaced verbatim, got: {json_after}"
    );

    let text_after = status_output(repo.path(), false, false, false).unwrap();
    assert!(
        text_after.contains("repair audit: unavailable"),
        "marker must flip human output to unavailable, got: {text_after}"
    );
    assert!(
        text_after.contains("2099-01-01T00:00:00Z"),
        "human output must include the failure timestamp, got: {text_after}"
    );
    assert!(
        text_after.contains("open failed: test injection"),
        "human output must include the failure reason, got: {text_after}"
    );
}

#[test]
fn append_resolution_log_clears_degraded_marker_on_success() {
    let repo = tempdir().unwrap();
    let synrepo_dir = Config::synrepo_dir(repo.path());
    std::fs::create_dir_all(synrepo_dir.join("state")).unwrap();

    let marker_path = repair_log_degraded_marker_path(&synrepo_dir);
    std::fs::write(
        &marker_path,
        r#"{"last_failure_at":"","last_failure_reason":"stale"}"#,
    )
    .unwrap();
    assert!(marker_path.exists(), "marker must exist before the test");

    let entry = ResolutionLogEntry {
        synced_at: OffsetDateTime::now_utc().format(&Rfc3339).unwrap(),
        source_revision: None,
        requested_scope: vec![],
        findings_considered: vec![],
        actions_taken: vec![],
        outcome: SyncOutcome::Completed,
    };
    append_resolution_log(&synrepo_dir, &entry);

    assert!(
        !marker_path.exists(),
        "successful append must clear the degraded marker"
    );
}
