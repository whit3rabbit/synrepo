#![cfg(feature = "semantic-triage")]

use std::{sync::atomic::AtomicBool, time::Duration};

use tempfile::tempdir;

use super::scheduler::{EmbeddingRefreshScheduler, ReconcileEmbeddingObservation};
use crate::{
    config::Config,
    pipeline::{
        structural::CompileSummary,
        watch::{
            lease::WatchStateHandle, reconcile::ReconcileOutcome, watch_daemon_state_path,
            WatchDaemonState, WatchServiceMode,
        },
    },
};

fn config() -> Config {
    Config {
        enable_semantic_triage: true,
        ..Config::default()
    }
}

fn completed() -> ReconcileOutcome {
    ReconcileOutcome::Completed(CompileSummary::default())
}

fn observation<'a>(
    outcome: &'a ReconcileOutcome,
    triggering_events: usize,
    force_full_reconcile: bool,
    keepalive: bool,
) -> ReconcileEmbeddingObservation<'a> {
    ReconcileEmbeddingObservation {
        outcome,
        triggering_events,
        force_full_reconcile,
        keepalive,
    }
}

fn state_handle(synrepo_dir: &std::path::Path) -> WatchStateHandle {
    WatchStateHandle::new(
        watch_daemon_state_path(synrepo_dir),
        WatchDaemonState::new(synrepo_dir, WatchServiceMode::Foreground),
    )
}

fn write_index(synrepo_dir: &std::path::Path) {
    let index = synrepo_dir.join("index/vectors/index.bin");
    std::fs::create_dir_all(index.parent().unwrap()).unwrap();
    std::fs::write(index, b"placeholder").unwrap();
}

#[test]
fn marks_existing_index_stale_after_source_reconcile() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");
    write_index(&synrepo_dir);
    let handle = state_handle(&synrepo_dir);
    let mut scheduler =
        EmbeddingRefreshScheduler::for_test(Duration::ZERO, Duration::from_secs(300));

    scheduler.note_reconcile(
        &config(),
        &synrepo_dir,
        observation(&completed(), 1, false, false),
        &handle,
    );

    assert!(scheduler.stale_for_test());
    assert!(handle.snapshot().embedding_index_stale);
}

#[test]
fn ignores_keepalive_reconcile() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");
    write_index(&synrepo_dir);
    let handle = state_handle(&synrepo_dir);
    let mut scheduler =
        EmbeddingRefreshScheduler::for_test(Duration::ZERO, Duration::from_secs(300));

    scheduler.note_reconcile(
        &config(),
        &synrepo_dir,
        observation(&completed(), 0, false, true),
        &handle,
    );

    assert!(!scheduler.stale_for_test());
    assert!(!handle.snapshot().embedding_index_stale);
}

#[test]
fn waits_for_quiet_window_and_no_pending_changes() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");
    write_index(&synrepo_dir);
    let handle = state_handle(&synrepo_dir);
    let auto_sync = AtomicBool::new(true);
    let auto_sync_blocked = AtomicBool::new(false);
    let mut scheduler =
        EmbeddingRefreshScheduler::for_test(Duration::from_secs(30), Duration::from_secs(300));

    scheduler.note_reconcile(
        &config(),
        &synrepo_dir,
        observation(&completed(), 1, false, false),
        &handle,
    );

    assert!(!scheduler.should_start_for_test(
        &config(),
        &synrepo_dir,
        &auto_sync,
        &auto_sync_blocked,
        false,
    ));
    assert!(!scheduler.should_start_for_test(
        &config(),
        &synrepo_dir,
        &auto_sync,
        &auto_sync_blocked,
        true,
    ));
}

#[test]
fn requires_existing_index_and_auto_sync() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");
    let handle = state_handle(&synrepo_dir);
    let auto_sync = AtomicBool::new(false);
    let auto_sync_blocked = AtomicBool::new(false);
    let mut scheduler =
        EmbeddingRefreshScheduler::for_test(Duration::ZERO, Duration::from_secs(300));

    scheduler.note_reconcile(
        &config(),
        &synrepo_dir,
        observation(&completed(), 1, false, false),
        &handle,
    );
    assert!(!scheduler.stale_for_test());

    write_index(&synrepo_dir);
    scheduler.note_reconcile(
        &config(),
        &synrepo_dir,
        observation(&completed(), 1, false, false),
        &handle,
    );
    assert!(!scheduler.should_start_for_test(
        &config(),
        &synrepo_dir,
        &auto_sync,
        &auto_sync_blocked,
        false,
    ));
}

#[test]
fn respects_auto_sync_block_and_failure_backoff() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");
    write_index(&synrepo_dir);
    let handle = state_handle(&synrepo_dir);
    let auto_sync = AtomicBool::new(true);
    let auto_sync_blocked = AtomicBool::new(true);
    let mut scheduler =
        EmbeddingRefreshScheduler::for_test(Duration::ZERO, Duration::from_secs(300));

    scheduler.note_reconcile(
        &config(),
        &synrepo_dir,
        observation(&completed(), 1, false, false),
        &handle,
    );
    assert!(!scheduler.should_start_for_test(
        &config(),
        &synrepo_dir,
        &auto_sync,
        &auto_sync_blocked,
        false,
    ));

    auto_sync_blocked.store(false, std::sync::atomic::Ordering::Relaxed);
    scheduler.force_backoff_for_test(Duration::from_secs(300));
    assert!(!scheduler.should_start_for_test(
        &config(),
        &synrepo_dir,
        &auto_sync,
        &auto_sync_blocked,
        false,
    ));
}
