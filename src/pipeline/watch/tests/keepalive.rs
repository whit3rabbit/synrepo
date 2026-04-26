#![cfg(unix)]

use std::{thread, time::Duration};

use crate::pipeline::watch::{
    request_watch_control, run_watch_service, ReconcileOutcome, WatchConfig, WatchControlRequest,
    WatchEvent, WatchServiceMode,
};

use super::{setup_test_repo, watch_service_guard};

#[test]
fn watch_service_performs_periodic_keepalive_reconcile() {
    let _guard = watch_service_guard();
    let (_dir, repo, mut config, synrepo_dir) = setup_test_repo();

    config.reconcile_keepalive_seconds = 1;

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

    assert_startup_reconcile(&event_rx);
    assert_keepalive_reconcile(&event_rx);

    let _ = request_watch_control(&synrepo_dir, WatchControlRequest::Stop);
    handle.join().unwrap();
}

fn assert_startup_reconcile(event_rx: &crossbeam_channel::Receiver<WatchEvent>) {
    let first = event_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("startup ReconcileStarted must arrive");
    let second = event_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("startup ReconcileFinished must arrive");

    match (first, second) {
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
        ) => assert!(
            matches!(outcome, ReconcileOutcome::Completed(_)),
            "startup reconcile should complete in a fresh repo; got {:?}",
            outcome
        ),
        other => panic!("unexpected startup event pair: {:?}", other),
    }
}

fn assert_keepalive_reconcile(event_rx: &crossbeam_channel::Receiver<WatchEvent>) {
    let first = event_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("keepalive ReconcileStarted must arrive");
    let second = event_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("keepalive ReconcileFinished must arrive");

    match (first, second) {
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
        ) => assert!(
            matches!(outcome, ReconcileOutcome::Completed(_)),
            "keepalive reconcile should complete in a fresh repo; got {:?}",
            outcome
        ),
        other => panic!("unexpected keepalive event pair: {:?}", other),
    }
}
