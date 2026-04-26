#![cfg_attr(not(unix), allow(unused_imports, dead_code))]

use std::{
    fs,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex, MutexGuard,
    },
    thread,
    time::{Duration, Instant},
};

use crate::pipeline::watch::{
    load_reconcile_state, request_watch_control, run_watch_service, ReconcileOutcome, WatchConfig,
    WatchControlRequest, WatchControlResponse, WatchEvent, WatchServiceMode, WatchServiceStatus,
};

use super::{setup_test_repo, wait_for};

#[cfg(unix)]
static WATCH_SERVICE_TEST_LOCK: Mutex<()> = Mutex::new(());

#[cfg(unix)]
fn watch_service_guard() -> (MutexGuard<'static, ()>, crate::test_support::GlobalTestLock) {
    (
        WATCH_SERVICE_TEST_LOCK
            .lock()
            .expect("watch service test lock poisoned"),
        crate::test_support::global_test_lock("watch-service"),
    )
}

#[cfg(unix)]
#[test]
fn watch_service_handles_status_reconcile_and_stop() {
    let _guard = watch_service_guard();
    let (_dir, repo, config, synrepo_dir) = setup_test_repo();
    let service_repo = repo.clone();
    let service_config = config.clone();
    let service_synrepo = synrepo_dir.clone();

    let handle = thread::spawn(move || {
        run_watch_service(
            &service_repo,
            &service_config,
            &WatchConfig::default(),
            &service_synrepo,
            WatchServiceMode::Foreground,
            None,
        )
        .unwrap();
    });

    wait_for(
        || {
            matches!(
                super::super::watch_service_status(&synrepo_dir),
                WatchServiceStatus::Running(_)
            ) && super::super::watch_socket_path(&synrepo_dir).exists()
        },
        Duration::from_secs(5),
    );

    let status = request_watch_control(&synrepo_dir, WatchControlRequest::Status).unwrap();
    assert!(matches!(status, WatchControlResponse::Status { .. }));

    let reconcile = request_watch_control(&synrepo_dir, WatchControlRequest::ReconcileNow { fast: false }).unwrap();
    assert!(matches!(reconcile, WatchControlResponse::Reconcile { .. }));

    let stop = request_watch_control(&synrepo_dir, WatchControlRequest::Stop).unwrap();
    assert!(matches!(stop, WatchControlResponse::Ack { .. }));
    handle.join().unwrap();
}

#[test]
fn stop_bridge_acknowledges_without_waiting_for_loop_reply() {
    let stop_flag = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel();
    let started = Instant::now();

    let response = super::super::service::bridge_stop_request(&tx, &stop_flag);

    assert!(
        started.elapsed() < Duration::from_secs(1),
        "stop bridge should ack immediately"
    );
    assert!(matches!(response, WatchControlResponse::Ack { .. }));
    assert!(stop_flag.load(Ordering::Relaxed));
    assert!(
        rx.recv_timeout(Duration::from_secs(1)).is_ok(),
        "stop bridge should still wake the main loop"
    );
}

#[cfg(unix)]
#[test]
fn watch_service_records_lock_conflict_when_writer_lock_is_held() {
    let _guard = watch_service_guard();
    let (_dir, repo, config, synrepo_dir) = setup_test_repo();
    let service_repo = repo.clone();
    let service_config = config.clone();
    let service_synrepo = synrepo_dir.clone();

    let handle = thread::spawn(move || {
        run_watch_service(
            &service_repo,
            &service_config,
            &WatchConfig::default(),
            &service_synrepo,
            WatchServiceMode::Foreground,
            None,
        )
        .unwrap();
    });

    wait_for(
        || load_reconcile_state(&synrepo_dir).is_ok(),
        Duration::from_secs(3),
    );

    // Use a foreign PID to avoid re-entrancy in the same process. We also
    // actually hold the kernel advisory lock on a separate file description
    // so acquire_writer_lock sees a real conflict (not just stale metadata).
    let (mut child, foreign_pid) = super::live_foreign_pid();
    let lock_path = crate::pipeline::writer::writer_lock_path(&synrepo_dir);
    let owner = crate::pipeline::writer::WriterOwnership {
        pid: foreign_pid,
        acquired_at: crate::pipeline::writer::now_rfc3339(),
    };
    let _flock = crate::pipeline::writer::hold_writer_flock_with_ownership(&lock_path, &owner);

    let response = request_watch_control(&synrepo_dir, WatchControlRequest::ReconcileNow { fast: false }).unwrap();
    match response {
        WatchControlResponse::Reconcile { outcome, .. } => {
            assert_eq!(outcome.as_str(), "lock-conflict");
            if let ReconcileOutcome::LockConflict { holder_pid } = outcome {
                assert_eq!(holder_pid, foreign_pid);
            }
        }
        other => panic!("unexpected control response: {:?}", other),
    }

    child.kill().unwrap();

    let _ = request_watch_control(&synrepo_dir, WatchControlRequest::Stop);
    handle.join().unwrap();
}

#[cfg(unix)]
#[test]
fn watch_service_ignores_runtime_only_writes() {
    let _guard = watch_service_guard();
    let (_dir, repo, config, synrepo_dir) = setup_test_repo();
    let service_repo = repo.clone();
    let service_config = config.clone();
    let service_synrepo = synrepo_dir.clone();

    let handle = thread::spawn(move || {
        run_watch_service(
            &service_repo,
            &service_config,
            &WatchConfig::default(),
            &service_synrepo,
            WatchServiceMode::Foreground,
            None,
        )
        .unwrap();
    });

    wait_for(
        || {
            matches!(
                super::super::watch_service_status(&synrepo_dir),
                WatchServiceStatus::Running(_)
            ) && super::super::watch_socket_path(&synrepo_dir).exists()
        },
        Duration::from_secs(5),
    );
    wait_for(
        || match request_watch_control(&synrepo_dir, WatchControlRequest::Status) {
            Ok(WatchControlResponse::Status { snapshot }) => snapshot.last_reconcile_at.is_some(),
            _ => false,
        },
        Duration::from_secs(5),
    );

    let baseline = match request_watch_control(&synrepo_dir, WatchControlRequest::Status).unwrap() {
        WatchControlResponse::Status { snapshot } => snapshot,
        other => panic!("unexpected control response: {:?}", other),
    };

    fs::write(synrepo_dir.join("state/noise.txt"), "runtime only").unwrap();
    thread::sleep(Duration::from_millis(800));

    let after = match request_watch_control(&synrepo_dir, WatchControlRequest::Status).unwrap() {
        WatchControlResponse::Status { snapshot } => snapshot,
        other => panic!("unexpected control response: {:?}", other),
    };

    assert_eq!(after.last_event_at, baseline.last_event_at);
    assert_eq!(after.last_reconcile_at, baseline.last_reconcile_at);

    let _ = request_watch_control(&synrepo_dir, WatchControlRequest::Stop);
    handle.join().unwrap();
}

#[cfg(unix)]
#[test]
fn watch_service_emits_started_then_finished_events_for_startup_reconcile() {
    let _guard = watch_service_guard();
    let (_dir, repo, config, synrepo_dir) = setup_test_repo();
    let service_repo = repo.clone();
    let service_config = config.clone();
    let service_synrepo = synrepo_dir.clone();
    let (event_tx, event_rx) = crossbeam_channel::bounded::<WatchEvent>(16);

    let handle = thread::spawn(move || {
        run_watch_service(
            &service_repo,
            &service_config,
            &WatchConfig::default(),
            &service_synrepo,
            WatchServiceMode::Foreground,
            Some(event_tx),
        )
        .unwrap();
    });

    // Startup pass emits Started+Finished before any filesystem events land.
    let first = event_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("startup ReconcileStarted must arrive");
    let second = event_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("startup ReconcileFinished must arrive");

    match (&first, &second) {
        (
            WatchEvent::ReconcileStarted {
                triggering_events: 0,
                ..
            },
            WatchEvent::ReconcileFinished {
                outcome,
                triggering_events: 0,
                ..
            },
        ) => {
            assert!(
                matches!(outcome, ReconcileOutcome::Completed(_)),
                "startup reconcile should complete in a fresh repo; got {:?}",
                outcome
            );
        }
        other => panic!("unexpected startup event pair: {:?}", other),
    }

    let _ = request_watch_control(&synrepo_dir, WatchControlRequest::Stop);
    handle.join().unwrap();
}

#[cfg(unix)]
#[test]
fn watch_service_stop_stays_responsive_after_rapid_source_writes() {
    let _guard = watch_service_guard();
    let (_dir, repo, config, synrepo_dir) = setup_test_repo();
    let service_repo = repo.clone();
    let service_config = config.clone();
    let service_synrepo = synrepo_dir.clone();

    let handle = thread::spawn(move || {
        run_watch_service(
            &service_repo,
            &service_config,
            &WatchConfig::default(),
            &service_synrepo,
            WatchServiceMode::Foreground,
            None,
        )
        .unwrap();
    });

    wait_for(
        || {
            matches!(
                super::super::watch_service_status(&synrepo_dir),
                WatchServiceStatus::Running(_)
            ) && super::super::watch_socket_path(&synrepo_dir).exists()
        },
        Duration::from_secs(5),
    );

    for idx in 0..200 {
        fs::write(
            repo.join("src/lib.rs"),
            format!("pub fn hello_{idx}() -> usize {{ {idx} }}\n"),
        )
        .unwrap();
    }

    let started = Instant::now();
    let stop = request_watch_control(&synrepo_dir, WatchControlRequest::Stop).unwrap();
    assert!(matches!(stop, WatchControlResponse::Ack { .. }));
    handle.join().unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "stop should not sit behind an unbounded watch backlog"
    );
}

/// Regression guard for sync-watch-delegation-v1: the watch control socket
/// handles `SyncNow` (responding with a `Sync { summary }`) and `SetAutoSync`
/// (responding with an `Ack`). Without this test the asymmetry that prompted
/// the change package could silently return.
#[cfg(unix)]
#[test]
fn watch_service_handles_sync_now_and_set_auto_sync() {
    use crate::pipeline::repair::SyncOptions;

    let _guard = watch_service_guard();
    let (_dir, repo, config, synrepo_dir) = setup_test_repo();
    // Disable auto-sync for this test to prevent background auto-sync from
    // racing with the explicit `SyncNow` call.
    let mut config = config;
    config.auto_sync_enabled = false;

    let service_repo = repo.clone();
    let service_config = config.clone();
    let service_synrepo = synrepo_dir.clone();

    let handle = thread::spawn(move || {
        run_watch_service(
            &service_repo,
            &service_config,
            &WatchConfig::default(),
            &service_synrepo,
            WatchServiceMode::Foreground,
            None,
        )
        .unwrap();
    });

    wait_for(
        || {
            matches!(
                super::super::watch_service_status(&synrepo_dir),
                WatchServiceStatus::Running(_)
            ) && super::super::watch_socket_path(&synrepo_dir).exists()
        },
        Duration::from_secs(5),
    );

    let sync_response = request_watch_control(
        &synrepo_dir,
        WatchControlRequest::SyncNow {
            options: SyncOptions::default(),
        },
    )
    .unwrap();
    assert!(
        matches!(sync_response, WatchControlResponse::Sync { .. }),
        "expected Sync response, got {:?}",
        sync_response
    );

    let auto_off = request_watch_control(
        &synrepo_dir,
        WatchControlRequest::SetAutoSync { enabled: false },
    )
    .unwrap();
    match auto_off {
        WatchControlResponse::Ack { message } => assert!(message.contains("off")),
        other => panic!("expected Ack for SetAutoSync(off), got {:?}", other),
    }

    let auto_on = request_watch_control(
        &synrepo_dir,
        WatchControlRequest::SetAutoSync { enabled: true },
    )
    .unwrap();
    match auto_on {
        WatchControlResponse::Ack { message } => assert!(message.contains("on")),
        other => panic!("expected Ack for SetAutoSync(on), got {:?}", other),
    }

    let _ = request_watch_control(&synrepo_dir, WatchControlRequest::Stop);
    handle.join().unwrap();
}
