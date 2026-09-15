#![cfg(unix)]

use std::{thread, time::Duration};

use crate::pipeline::{
    repair::SyncOptions,
    watch::{
        request_watch_control, run_watch_service, WatchConfig, WatchControlRequest,
        WatchControlResponse, WatchEvent, WatchServiceMode, WatchServiceStatus,
    },
};

use super::{setup_test_repo, wait_for, watch_service_guard};

#[test]
fn blocked_startup_keeps_control_plane_responsive_and_lease_truthful() {
    let _guard = watch_service_guard();
    let (_dir, repo, config, synrepo_dir) = setup_test_repo();
    let (entered, release) = super::super::worker::install_test_gate(
        super::super::worker::WatchOperationKind::Reconcile,
    );
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

    entered.recv_timeout(Duration::from_secs(5)).unwrap();
    wait_for(
        || super::super::watch_socket_path(&synrepo_dir).exists(),
        Duration::from_secs(5),
    );
    assert!(matches!(
        request_watch_control(&synrepo_dir, WatchControlRequest::Status).unwrap(),
        WatchControlResponse::Status { .. }
    ));
    assert!(matches!(
        request_watch_control(
            &synrepo_dir,
            WatchControlRequest::SuppressPaths {
                paths: vec![repo.join("src/lib.rs")],
                ttl_ms: 1_000,
            },
        )
        .unwrap(),
        WatchControlResponse::Ack { .. }
    ));
    assert!(matches!(
        request_watch_control(
            &synrepo_dir,
            WatchControlRequest::SetAutoSync { enabled: false },
        )
        .unwrap(),
        WatchControlResponse::Ack { .. }
    ));
    assert!(matches!(
        request_watch_control(
            &synrepo_dir,
            WatchControlRequest::SyncNow {
                options: SyncOptions::default(),
            },
        )
        .unwrap(),
        WatchControlResponse::Error { message } if message.contains("busy with reconcile")
    ));

    assert!(matches!(
        request_watch_control(&synrepo_dir, WatchControlRequest::Stop).unwrap(),
        WatchControlResponse::Ack { .. }
    ));
    assert!(matches!(
        super::super::watch_service_status(&synrepo_dir),
        WatchServiceStatus::Running(_)
    ));
    release.send(()).unwrap();
    handle.join().unwrap();
}

#[test]
fn blocked_manual_mutations_share_one_slot_and_stop_cleanly() {
    use super::super::worker::WatchOperationKind;

    let _guard = watch_service_guard();
    for kind in [
        WatchOperationKind::Reconcile,
        WatchOperationKind::Sync,
        WatchOperationKind::Embeddings,
    ] {
        let (_dir, repo, mut config, synrepo_dir) = setup_test_repo();
        config.auto_sync_enabled = false;
        let service_repo = repo.clone();
        let service_config = config.clone();
        let service_synrepo = synrepo_dir.clone();
        let service = thread::spawn(move || {
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
                super::super::watch_socket_path(&synrepo_dir).exists()
                    && super::super::load_reconcile_state(&synrepo_dir).is_ok()
            },
            Duration::from_secs(5),
        );

        let (entered, release) = super::super::worker::install_test_gate(kind);
        let request_dir = synrepo_dir.clone();
        let request = thread::spawn(move || loop {
            let response = request_watch_control(&request_dir, request_for(kind)).unwrap();
            if !matches!(
                &response,
                WatchControlResponse::Error { message } if message.contains("busy with")
            ) {
                return response;
            }
            thread::sleep(Duration::from_millis(25));
        });
        entered.recv_timeout(Duration::from_secs(5)).unwrap();

        let second = request_watch_control(
            &synrepo_dir,
            WatchControlRequest::SyncNow {
                options: SyncOptions::default(),
            },
        )
        .unwrap();
        assert!(matches!(
            second,
            WatchControlResponse::Error { message }
                if message.contains(&format!("busy with {}", kind.as_str()))
        ));
        assert!(matches!(
            request_watch_control(&synrepo_dir, WatchControlRequest::Status).unwrap(),
            WatchControlResponse::Status { .. }
        ));
        assert!(matches!(
            request_watch_control(&synrepo_dir, WatchControlRequest::Stop).unwrap(),
            WatchControlResponse::Ack { .. }
        ));
        assert!(matches!(
            super::super::watch_service_status(&synrepo_dir),
            WatchServiceStatus::Running(_)
        ));

        release.send(()).unwrap();
        assert!(matches!(
            request.join().unwrap(),
            WatchControlResponse::Error { message } if message.contains("stopping")
        ));
        service.join().unwrap();
    }
}

#[test]
fn filesystem_changes_waiting_behind_manual_work_reconcile_afterward() {
    let _guard = watch_service_guard();
    let (_dir, repo, mut config, synrepo_dir) = setup_test_repo();
    config.auto_sync_enabled = false;
    let service_repo = repo.clone();
    let service_config = config.clone();
    let service_synrepo = synrepo_dir.clone();
    let (event_tx, event_rx) = crossbeam_channel::bounded(64);
    let service = thread::spawn(move || {
        run_watch_service(
            &service_repo,
            &service_config,
            &WatchConfig {
                debounce_timeout: Duration::from_millis(50),
                max_events_per_cycle: 1_000,
            },
            &service_synrepo,
            WatchServiceMode::Foreground,
            Some(event_tx),
        )
        .unwrap();
    });
    wait_for(
        || {
            super::super::watch_socket_path(&synrepo_dir).exists()
                && super::super::load_reconcile_state(&synrepo_dir).is_ok()
        },
        Duration::from_secs(5),
    );

    let (entered, release) =
        super::super::worker::install_test_gate(super::super::worker::WatchOperationKind::Sync);
    let request_dir = synrepo_dir.clone();
    let request = thread::spawn(move || loop {
        let response = request_watch_control(
            &request_dir,
            WatchControlRequest::SyncNow {
                options: SyncOptions::default(),
            },
        )
        .unwrap();
        if !matches!(
            &response,
            WatchControlResponse::Error { message } if message.contains("busy with")
        ) {
            return response;
        }
        thread::sleep(Duration::from_millis(25));
    });
    entered.recv_timeout(Duration::from_secs(5)).unwrap();
    std::fs::write(repo.join("src/queued.rs"), "pub fn queued() {}\n").unwrap();
    thread::sleep(Duration::from_millis(150));
    release.send(()).unwrap();
    assert!(matches!(
        request.join().unwrap(),
        WatchControlResponse::Sync { .. }
    ));

    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut observed = false;
    while std::time::Instant::now() < deadline {
        if let Ok(WatchEvent::ReconcileFinished {
            triggering_events, ..
        }) = event_rx.recv_timeout(Duration::from_millis(100))
        {
            if triggering_events > 0 {
                observed = true;
                break;
            }
        }
    }
    assert!(observed, "queued filesystem change was not reconciled");

    let _ = request_watch_control(&synrepo_dir, WatchControlRequest::Stop);
    service.join().unwrap();
}

fn request_for(kind: super::super::worker::WatchOperationKind) -> WatchControlRequest {
    match kind {
        super::super::worker::WatchOperationKind::Reconcile => {
            WatchControlRequest::ReconcileNow { fast: false }
        }
        super::super::worker::WatchOperationKind::Sync => WatchControlRequest::SyncNow {
            options: SyncOptions::default(),
        },
        super::super::worker::WatchOperationKind::Embeddings => {
            WatchControlRequest::EmbeddingsBuildNow
        }
    }
}
