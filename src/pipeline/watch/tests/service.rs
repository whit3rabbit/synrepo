#![cfg_attr(not(unix), allow(unused_imports, dead_code))]

use std::{fs, thread, time::Duration};

use notify_debouncer_full::{
    notify::{event::ModifyKind, Event, EventKind},
    DebouncedEvent,
};

use crate::pipeline::watch::{
    load_reconcile_state, request_watch_control, run_watch_service, ReconcileOutcome, WatchConfig,
    WatchControlRequest, WatchControlResponse, WatchServiceMode, WatchServiceStatus,
};

use super::{setup_test_repo, wait_for};

#[test]
fn filter_repo_events_ignores_synrepo_only_bursts() {
    let (_dir, repo, _config, synrepo_dir) = setup_test_repo();
    let runtime_event = DebouncedEvent::from(
        Event::new(EventKind::Modify(ModifyKind::Any))
            .add_path(synrepo_dir.join("state/watch-daemon.json"))
            .add_path(repo.clone()),
    );
    let source_event = DebouncedEvent::from(
        Event::new(EventKind::Modify(ModifyKind::Any)).add_path(repo.join("src/lib.rs")),
    );

    let filtered =
        super::super::service::filter_repo_events(vec![runtime_event, source_event], &synrepo_dir);
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].paths[0], repo.join("src/lib.rs"));
}

#[cfg(unix)]
#[test]
fn watch_service_handles_status_reconcile_and_stop() {
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

    let reconcile = request_watch_control(&synrepo_dir, WatchControlRequest::ReconcileNow).unwrap();
    assert!(matches!(reconcile, WatchControlResponse::Reconcile { .. }));

    let stop = request_watch_control(&synrepo_dir, WatchControlRequest::Stop).unwrap();
    assert!(matches!(stop, WatchControlResponse::Ack { .. }));
    handle.join().unwrap();
}

#[cfg(unix)]
#[test]
fn watch_service_records_lock_conflict_when_writer_lock_is_held() {
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

    let response = request_watch_control(&synrepo_dir, WatchControlRequest::ReconcileNow).unwrap();
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
