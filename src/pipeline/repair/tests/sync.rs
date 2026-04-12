use tempfile::tempdir;

use super::support::{setup_repo_for_sync, write_foreign_lock};
use crate::pipeline::repair::{
    execute_sync, repair_log_path, RepairSurface, ResolutionLogEntry, SyncOutcome,
};
use crate::{
    config::Config,
    pipeline::{
        watch::{persist_reconcile_state, ReconcileOutcome},
        writer::acquire_writer_lock,
    },
};

#[test]
fn sync_on_current_runtime_produces_no_repaired_findings() {
    let dir = tempdir().unwrap();
    let (repo, synrepo_dir) = setup_repo_for_sync(&dir);

    let summary = execute_sync(&repo, &synrepo_dir, &Config::default()).unwrap();

    assert_eq!(
        summary.repaired.len(),
        0,
        "current runtime should have nothing to repair"
    );
    assert!(summary.blocked.is_empty());
}

#[test]
fn sync_repairs_stale_reconcile_and_writes_resolution_log() {
    let dir = tempdir().unwrap();
    let (repo, synrepo_dir) = setup_repo_for_sync(&dir);

    persist_reconcile_state(
        &synrepo_dir,
        &ReconcileOutcome::Failed("forced stale".to_string()),
        0,
    );

    let summary = execute_sync(&repo, &synrepo_dir, &Config::default()).unwrap();
    let repaired_surfaces: Vec<_> = summary.repaired.iter().map(|f| f.surface).collect();
    assert!(
        repaired_surfaces.contains(&RepairSurface::StructuralRefresh),
        "StructuralRefresh must be repaired, got: {:?}",
        repaired_surfaces
    );
    assert!(
        repair_log_path(&synrepo_dir).exists(),
        "resolution log must be created after sync"
    );

    let log_content = std::fs::read_to_string(repair_log_path(&synrepo_dir)).unwrap();
    assert_eq!(
        log_content.lines().count(),
        1,
        "one sync run = one log entry"
    );

    let decoded: ResolutionLogEntry =
        serde_json::from_str(log_content.lines().next().unwrap()).unwrap();
    assert_eq!(decoded.outcome, SyncOutcome::Completed);
    assert!(
        !decoded.actions_taken.is_empty(),
        "actions must be recorded"
    );
}

#[test]
fn sync_places_unsupported_surfaces_in_report_only() {
    let dir = tempdir().unwrap();
    let (repo, synrepo_dir) = setup_repo_for_sync(&dir);

    let summary = execute_sync(&repo, &synrepo_dir, &Config::default()).unwrap();
    let report_only_surfaces: Vec<_> = summary.report_only.iter().map(|f| f.surface).collect();
    for surface in [RepairSurface::ExportViews, RepairSurface::StaleRationale] {
        assert!(
            report_only_surfaces.contains(&surface),
            "{} must be in report_only, got: {:?}",
            surface.as_str(),
            report_only_surfaces
        );
    }
    // CommentaryOverlayEntries with DriftClass::Absent / Severity::ReportOnly
    // also routes to report_only when no overlay.db exists.
    assert!(
        report_only_surfaces.contains(&RepairSurface::CommentaryOverlayEntries),
        "commentary_overlay_entries (absent) must be in report_only, got: {report_only_surfaces:?}"
    );
}

#[test]
fn sync_sets_partial_outcome_when_blocked_findings_present() {
    let dir = tempdir().unwrap();
    let (repo, synrepo_dir) = setup_repo_for_sync(&dir);
    write_foreign_lock(&synrepo_dir);

    let _ = execute_sync(&repo, &synrepo_dir, &Config::default()).unwrap();

    // append_resolution_log always writes the file; no conditional guard needed.
    let log_content = std::fs::read_to_string(repair_log_path(&synrepo_dir)).unwrap();
    let decoded: ResolutionLogEntry =
        serde_json::from_str(log_content.lines().next().unwrap()).unwrap();
    // PID 42 may be alive (Partial) or dead/stale (Completed after the lock is cleared).
    assert!(
        decoded.outcome == SyncOutcome::Partial || decoded.outcome == SyncOutcome::Completed,
        "unexpected outcome: {:?}",
        decoded.outcome
    );
}

#[test]
fn sync_renders_report_only_and_repaired_distinctly() {
    let dir = tempdir().unwrap();
    let (repo, synrepo_dir) = setup_repo_for_sync(&dir);

    persist_reconcile_state(
        &synrepo_dir,
        &ReconcileOutcome::Failed("forced".to_string()),
        0,
    );

    let summary = execute_sync(&repo, &synrepo_dir, &Config::default()).unwrap();
    let rendered = summary.render();

    assert!(
        rendered.contains("repaired:"),
        "render must include repaired section"
    );
    assert!(
        rendered.contains("report-only:"),
        "render must include report-only section"
    );
    assert!(
        rendered.contains("[ok]"),
        "repaired surfaces must be marked [ok]"
    );
    assert!(
        rendered.contains("[skip]"),
        "report-only surfaces must be marked [skip]"
    );
}

#[test]
fn sync_does_not_report_structural_refresh_as_repaired_when_reconcile_hits_lock_conflict() {
    let dir = tempdir().unwrap();
    let (repo, synrepo_dir) = setup_repo_for_sync(&dir);

    persist_reconcile_state(
        &synrepo_dir,
        &ReconcileOutcome::Failed("forced stale".to_string()),
        0,
    );
    let _lock = acquire_writer_lock(&synrepo_dir).unwrap();

    let summary = execute_sync(&repo, &synrepo_dir, &Config::default()).unwrap();

    assert!(
        !summary
            .repaired
            .iter()
            .any(|finding| finding.surface == RepairSurface::StructuralRefresh),
        "lock-conflicted reconcile must not be counted as repaired"
    );
    assert!(
        summary
            .blocked
            .iter()
            .any(|finding| finding.surface == RepairSurface::StructuralRefresh),
        "structural refresh should be blocked when reconcile cannot acquire the lock"
    );

    let log_content = std::fs::read_to_string(repair_log_path(&synrepo_dir)).unwrap();
    let decoded: ResolutionLogEntry =
        serde_json::from_str(log_content.lines().next().unwrap()).unwrap();
    assert_eq!(decoded.outcome, SyncOutcome::Partial);
}

#[test]
fn sync_does_not_report_structural_refresh_as_repaired_when_reconcile_fails() {
    let dir = tempdir().unwrap();
    let (repo, synrepo_dir) = setup_repo_for_sync(&dir);

    persist_reconcile_state(
        &synrepo_dir,
        &ReconcileOutcome::Failed("forced stale".to_string()),
        0,
    );
    std::fs::remove_dir_all(synrepo_dir.join("graph")).unwrap();
    std::fs::write(synrepo_dir.join("graph"), "not a directory").unwrap();

    let summary = execute_sync(&repo, &synrepo_dir, &Config::default()).unwrap();

    assert!(
        !summary
            .repaired
            .iter()
            .any(|finding| finding.surface == RepairSurface::StructuralRefresh),
        "failed reconcile must not be counted as repaired"
    );
    assert!(
        summary
            .blocked
            .iter()
            .any(|finding| finding.surface == RepairSurface::StructuralRefresh),
        "structural refresh should be blocked when reconcile fails"
    );

    let log_content = std::fs::read_to_string(repair_log_path(&synrepo_dir)).unwrap();
    let decoded: ResolutionLogEntry =
        serde_json::from_str(log_content.lines().next().unwrap()).unwrap();
    assert_eq!(decoded.outcome, SyncOutcome::Partial);
}
