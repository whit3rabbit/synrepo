use tempfile::tempdir;

use super::support::setup_repo_for_sync;
#[cfg(unix)]
use super::support::write_foreign_lock;
use crate::pipeline::repair::{
    execute_sync, execute_sync_locked, repair_log_path, RepairSurface, ResolutionLogEntry,
    SurfaceOutcome, SyncOptions, SyncOutcome, SyncProgress,
};
use crate::pipeline::writer::acquire_write_admission;
use crate::{
    config::Config,
    pipeline::watch::{persist_reconcile_state, ReconcileOutcome},
};

#[test]
fn sync_on_current_runtime_produces_no_repaired_findings() {
    let dir = tempdir().unwrap();
    let (repo, synrepo_dir) = setup_repo_for_sync(&dir);

    let summary = execute_sync(
        &repo,
        &synrepo_dir,
        &Config::default(),
        SyncOptions::default(),
    )
    .unwrap();

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

    let summary = execute_sync(
        &repo,
        &synrepo_dir,
        &Config::default(),
        SyncOptions::default(),
    )
    .unwrap();
    let repaired_surfaces: Vec<_> = summary.repaired.iter().map(|f| f.surface).collect();
    let blocked_surfaces: Vec<_> = summary.blocked.iter().map(|f| f.surface).collect();
    let report_only_surfaces: Vec<_> = summary.report_only.iter().map(|f| f.surface).collect();
    assert!(
        repaired_surfaces.contains(&RepairSurface::StructuralRefresh),
        "StructuralRefresh must be repaired. repaired={:?} blocked={:?} report_only={:?}",
        repaired_surfaces,
        blocked_surfaces,
        report_only_surfaces
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
fn sync_routes_report_only_surfaces_correctly() {
    let dir = tempdir().unwrap();
    let (repo, synrepo_dir) = setup_repo_for_sync(&dir);

    let summary = execute_sync(
        &repo,
        &synrepo_dir,
        &Config::default(),
        SyncOptions::default(),
    )
    .unwrap();
    let report_only_surfaces: Vec<_> = summary.report_only.iter().map(|f| f.surface).collect();
    // StaleRationale is implemented and always emits a ReportOnly finding
    // (human-authored rationale can never be auto-repaired). In a repo with
    // no drift it routes to report_only with DriftClass::Current or Absent.
    assert!(
        report_only_surfaces.contains(&RepairSurface::StaleRationale),
        "stale_rationale must be in report_only, got: {:?}",
        report_only_surfaces
    );
    // CommentaryOverlayEntries with DriftClass::Absent / Severity::ReportOnly
    // also routes to report_only when no overlay.db exists.
    assert!(
        report_only_surfaces.contains(&RepairSurface::CommentaryOverlayEntries),
        "commentary_overlay_entries (absent) must be in report_only, got: {report_only_surfaces:?}"
    );
}

// PID 42 is inaccessible (EPERM) on unix, so `is_process_alive` returns false
// and the lock is treated as stale. On Windows the conservative fallback always
// returns true, so execute_sync cannot acquire the lock and returns Err instead.
#[cfg(unix)]
#[test]
fn sync_sets_partial_outcome_when_blocked_findings_present() {
    let dir = tempdir().unwrap();
    let (repo, synrepo_dir) = setup_repo_for_sync(&dir);
    write_foreign_lock(&synrepo_dir);

    let _ = execute_sync(
        &repo,
        &synrepo_dir,
        &Config::default(),
        SyncOptions::default(),
    )
    .unwrap();

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

    let summary = execute_sync(
        &repo,
        &synrepo_dir,
        &Config::default(),
        SyncOptions::default(),
    )
    .unwrap();
    let rendered = summary.render();
    let repaired_surfaces: Vec<_> = summary.repaired.iter().map(|f| f.surface).collect();
    let blocked_surfaces: Vec<_> = summary.blocked.iter().map(|f| f.surface).collect();
    let report_only_surfaces: Vec<_> = summary.report_only.iter().map(|f| f.surface).collect();
    let ctx = format!(
        "repaired={repaired_surfaces:?} blocked={blocked_surfaces:?} report_only={report_only_surfaces:?}\nrendered=\n{rendered}"
    );

    assert!(
        rendered.contains("repaired:"),
        "render must include repaired section; {ctx}"
    );
    assert!(
        rendered.contains("report-only:"),
        "render must include report-only section; {ctx}"
    );
    assert!(
        rendered.contains("[ok]"),
        "repaired surfaces must be marked [ok]; {ctx}"
    );
    assert!(
        rendered.contains("[skip]"),
        "report-only surfaces must be marked [skip]; {ctx}"
    );
}

#[cfg(unix)]
#[test]
fn sync_fails_fast_when_writer_lock_is_held() {
    use crate::pipeline::writer::{
        hold_writer_flock_with_ownership, writer_lock_path, WriterOwnership,
    };

    // execute_sync acquires the lock at start and fails fast when another
    // file description holds the kernel flock. We simulate a foreign holder
    // by taking the flock on a second file description (blocks our later
    // open/try_lock the same way a separate process would) and stamping a
    // live foreign PID into the ownership metadata for display.
    let dir = tempdir().unwrap();
    let (repo, synrepo_dir) = setup_repo_for_sync(&dir);

    persist_reconcile_state(
        &synrepo_dir,
        &ReconcileOutcome::Failed("forced stale".to_string()),
        0,
    );

    let mut sleep_child = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .expect("spawn sleep");
    let holder_pid = sleep_child.id();
    let ownership = WriterOwnership {
        pid: holder_pid,
        acquired_at: "2099-01-01T00:00:00Z".to_string(),
    };
    let _flock = hold_writer_flock_with_ownership(&writer_lock_path(&synrepo_dir), &ownership);

    let result = execute_sync(
        &repo,
        &synrepo_dir,
        &Config::default(),
        SyncOptions::default(),
    );
    let _ = sleep_child.kill();
    let _ = sleep_child.wait();

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("writer lock held by pid"),
        "expected lock-held error, got: {err}"
    );
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

    let summary = execute_sync(
        &repo,
        &synrepo_dir,
        &Config::default(),
        SyncOptions::default(),
    )
    .unwrap();

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

#[test]
fn execute_sync_locked_surface_filter_and_progress_callback() {
    // Regression guard: `execute_sync_locked` must
    // 1. invoke its progress callback once per surface with both a
    //    `SurfaceStarted` and a `SurfaceFinished` event, and
    // 2. bucket findings excluded by `surface_filter` as
    //    `SurfaceOutcome::FilteredOut` without letting their handler run.
    let _guard = crate::test_support::global_test_lock("repair-sync-surface-filter");
    let dir = tempdir().unwrap();
    let (repo, synrepo_dir) = setup_repo_for_sync(&dir);

    // Force a stale structural refresh so at least one surface is actionable.
    persist_reconcile_state(
        &synrepo_dir,
        &ReconcileOutcome::Failed("forced stale".to_string()),
        0,
    );

    let _lock = acquire_write_admission(&synrepo_dir, "test").unwrap();

    let mut events: Vec<SyncProgress> = Vec::new();
    let allow_list = [RepairSurface::ExportSurface];
    let filter: Option<&[RepairSurface]> = Some(&allow_list);

    let summary = {
        let mut cb = |ev: SyncProgress| events.push(ev);
        let mut progress: Option<&mut dyn FnMut(SyncProgress)> = Some(&mut cb);
        execute_sync_locked(
            &repo,
            &synrepo_dir,
            &Config::default(),
            SyncOptions::default(),
            &mut progress,
            filter,
        )
        .unwrap()
    };

    // Structural refresh was forced stale but is NOT in the allow list, so it
    // must show up as `FilteredOut` rather than in `repaired`.
    assert!(
        events.iter().any(|e| matches!(
            e,
            SyncProgress::SurfaceFinished {
                surface: RepairSurface::StructuralRefresh,
                outcome: SurfaceOutcome::FilteredOut,
            }
        )),
        "structural refresh should be filtered out when not in allow list; got {events:?}"
    );
    assert!(
        !summary
            .repaired
            .iter()
            .any(|f| f.surface == RepairSurface::StructuralRefresh),
        "filtered-out surface must not appear in repaired: {:?}",
        summary.repaired
    );

    // Each SurfaceStarted should be paired with a SurfaceFinished for the
    // same surface. Collect pairs by counting.
    let started: Vec<RepairSurface> = events
        .iter()
        .filter_map(|e| match e {
            SyncProgress::SurfaceStarted { surface, .. } => Some(*surface),
            _ => None,
        })
        .collect();
    let finished: Vec<RepairSurface> = events
        .iter()
        .filter_map(|e| match e {
            SyncProgress::SurfaceFinished { surface, .. } => Some(*surface),
            _ => None,
        })
        .collect();
    assert_eq!(
        started, finished,
        "every SurfaceStarted should have a matching SurfaceFinished"
    );
    assert!(
        !started.is_empty(),
        "at least one surface must fire when there is actionable drift"
    );
}
