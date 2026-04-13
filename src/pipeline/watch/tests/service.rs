use std::{fs, thread, time::Duration};

use notify_debouncer_full::{
    notify::{event::ModifyKind, Event, EventKind},
    DebouncedEvent,
};

use crate::pipeline::{
    watch::{
        load_reconcile_state, request_watch_control, run_watch_service, WatchConfig,
        WatchControlRequest, WatchControlResponse, WatchServiceMode, WatchServiceStatus,
    },
    writer::acquire_writer_lock,
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
            )
        },
        Duration::from_secs(3),
    );

    let status = request_watch_control(&synrepo_dir, WatchControlRequest::Status).unwrap();
    assert!(matches!(status, WatchControlResponse::Status { .. }));

    let reconcile = request_watch_control(&synrepo_dir, WatchControlRequest::ReconcileNow).unwrap();
    assert!(matches!(reconcile, WatchControlResponse::Status { .. }));

    let stop = request_watch_control(&synrepo_dir, WatchControlRequest::Stop).unwrap();
    assert!(matches!(stop, WatchControlResponse::Ack { .. }));
    handle.join().unwrap();
}

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
        || {
            matches!(
                super::super::watch_service_status(&synrepo_dir),
                WatchServiceStatus::Running(_)
            )
        },
        Duration::from_secs(3),
    );
    wait_for(
        || load_reconcile_state(&synrepo_dir).is_some(),
        Duration::from_secs(3),
    );

    let _lock = acquire_writer_lock(&synrepo_dir).unwrap();
    let response = request_watch_control(&synrepo_dir, WatchControlRequest::ReconcileNow).unwrap();
    match response {
        WatchControlResponse::Status { snapshot } => {
            assert_eq!(
                snapshot.last_reconcile_outcome.as_deref(),
                Some("lock-conflict")
            );
        }
        other => panic!("unexpected control response: {:?}", other),
    }

    let _ = request_watch_control(&synrepo_dir, WatchControlRequest::Stop);
    handle.join().unwrap();
}

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
            )
        },
        Duration::from_secs(3),
    );
    wait_for(
        || match request_watch_control(&synrepo_dir, WatchControlRequest::Status) {
            Ok(WatchControlResponse::Status { snapshot }) => snapshot.last_reconcile_at.is_some(),
            _ => false,
        },
        Duration::from_secs(3),
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
