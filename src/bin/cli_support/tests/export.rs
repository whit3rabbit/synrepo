//! CLI-level tests for the export command's write-admission gating.
//!
//! Mirrors the pattern used in `src/pipeline/writer/tests.rs` for
//! `acquire_write_admission`, but exercises the export CLI entry point so we
//! verify the admission guard is actually wired.

use std::fs;

use synrepo::bootstrap::bootstrap;
use synrepo::config::Config;
use synrepo::pipeline::export::ExportFormat;
#[cfg(not(unix))]
use synrepo::pipeline::writer::spawn_and_reap_pid;
#[cfg(unix)]
use synrepo::pipeline::writer::{
    hold_writer_flock_with_ownership, live_foreign_pid, spawn_and_reap_pid, writer_lock_path,
    WriterOwnership,
};
use tempfile::tempdir;

use crate::{export, sync};

fn write_watch_state(state_dir: &std::path::Path, pid: u32) {
    fs::create_dir_all(state_dir).unwrap();
    let state = serde_json::json!({
        "pid": pid,
        "started_at": "2026-01-01T00:00:00Z",
        "mode": "daemon",
        "socket_path": state_dir.join("watch.sock").display().to_string(),
    });
    fs::write(
        state_dir.join("watch-daemon.json"),
        serde_json::to_string(&state).unwrap(),
    )
    .unwrap();
}

#[test]
fn export_blocked_when_watch_running() {
    let dir = tempdir().unwrap();
    let repo = dir.path();
    bootstrap(repo, None).unwrap();

    let synrepo_dir = Config::synrepo_dir(repo);
    let state_dir = synrepo_dir.join("state");
    write_watch_state(&state_dir, std::process::id());

    let err = export(repo, ExportFormat::Markdown, false, false, None).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("watch service is active"),
        "expected watch-active message, got: {msg}"
    );

    // The writer guard must short-circuit before any export artifact lands.
    let export_dir = repo.join("synrepo-context");
    assert!(
        !export_dir.exists(),
        "export directory must not be created when admission is denied"
    );
}

#[cfg(unix)]
#[test]
fn export_succeeds_after_stale_watch_cleanup() {
    let dir = tempdir().unwrap();
    let repo = dir.path();
    bootstrap(repo, None).unwrap();

    let synrepo_dir = Config::synrepo_dir(repo);
    let state_dir = synrepo_dir.join("state");
    write_watch_state(&state_dir, spawn_and_reap_pid());

    // Stale watch-daemon.json with a dead PID must be cleaned up by
    // acquire_write_admission and the export must proceed.
    export(repo, ExportFormat::Markdown, false, false, None).expect("export must succeed");

    let export_dir = repo.join("synrepo-context");
    assert!(
        export_dir.join(".export-manifest.json").exists(),
        "manifest must be written after successful export"
    );
}

/// Both `export` and `sync` funnel through `acquire_write_admission`. If a
/// foreign writer holds the flock (simulating a concurrent `sync` in flight),
/// export must reject immediately with the lock-held error and must not
/// create the export directory. This guards the invariant that export and
/// sync serialize on the same admission gate; a future refactor that bypassed
/// the gate on one path would break this test.
#[cfg(unix)]
#[test]
fn export_blocked_when_writer_flock_held_by_foreign_process() {
    let dir = tempdir().unwrap();
    let repo = dir.path();
    bootstrap(repo, None).unwrap();

    let synrepo_dir = Config::synrepo_dir(repo);
    let lock_path = writer_lock_path(&synrepo_dir);

    let (mut child, pid) = live_foreign_pid();
    let _flock = hold_writer_flock_with_ownership(
        &lock_path,
        &WriterOwnership {
            pid,
            acquired_at: "2026-01-01T00:00:00Z".to_string(),
        },
    );

    let err = export(repo, ExportFormat::Markdown, false, false, None).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("writer lock held by pid"),
        "expected lock-held error, got: {msg}"
    );

    // Short-circuit happens before any render pass reaches the filesystem.
    let export_dir = repo.join("synrepo-context");
    assert!(
        !export_dir.exists(),
        "export directory must not be created while the writer lock is held"
    );

    // Release the flock and the foreign PID, then verify export succeeds.
    drop(_flock);
    let _ = child.kill();
    let _ = child.wait();

    export(repo, ExportFormat::Markdown, false, false, None).expect("export must succeed");
    assert!(
        export_dir.join(".export-manifest.json").exists(),
        "manifest must be written after the foreign lock is released"
    );
}

/// Counterpart to `export_blocked_when_writer_flock_held_by_foreign_process`:
/// pins that `sync` uses the same admission gate. If someone ever bypasses
/// `acquire_write_admission` for sync, this test fires.
///
/// We only assert the blocked path here. The clear-path recovery is not
/// symmetric with export because `sync` builds a repair report before
/// acquiring admission, and that report independently diagnoses the stale
/// lock file left on disk by the test holder. That behavior is tangential to
/// the admission-gate invariant this test guards.
#[cfg(unix)]
#[test]
fn sync_blocked_when_writer_flock_held_by_foreign_process() {
    let dir = tempdir().unwrap();
    let repo = dir.path();
    bootstrap(repo, None).unwrap();

    let synrepo_dir = Config::synrepo_dir(repo);
    let lock_path = writer_lock_path(&synrepo_dir);

    let (mut child, pid) = live_foreign_pid();
    let flock = hold_writer_flock_with_ownership(
        &lock_path,
        &WriterOwnership {
            pid,
            acquired_at: "2026-01-01T00:00:00Z".to_string(),
        },
    );

    let err = sync(repo, false, false, false).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("writer lock held by pid"),
        "expected lock-held error from sync, got: {msg}"
    );

    drop(flock);
    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn export_does_not_leave_partial_output_when_admission_fails() {
    let dir = tempdir().unwrap();
    let repo = dir.path();
    bootstrap(repo, None).unwrap();

    // Pre-create the export directory to verify the admission failure
    // short-circuits BEFORE any write touches it.
    let export_dir = repo.join("synrepo-context");
    fs::create_dir_all(&export_dir).unwrap();
    let sentinel = export_dir.join("sentinel.txt");
    fs::write(&sentinel, "pre-existing content").unwrap();

    let synrepo_dir = Config::synrepo_dir(repo);
    let state_dir = synrepo_dir.join("state");
    write_watch_state(&state_dir, std::process::id());

    let err = export(repo, ExportFormat::Markdown, false, false, None).unwrap_err();
    assert!(err.to_string().contains("watch service is active"));

    // Sentinel untouched: no render pass reached the filesystem.
    let content = fs::read_to_string(&sentinel).unwrap();
    assert_eq!(
        content, "pre-existing content",
        "export must not overwrite or partially populate the export directory on admission failure"
    );
    assert!(
        !export_dir.join(".export-manifest.json").exists(),
        "manifest must not be written when admission is denied"
    );
}
