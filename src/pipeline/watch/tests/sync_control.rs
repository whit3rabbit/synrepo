#![cfg(unix)]

use std::{thread, time::Duration};

use crate::pipeline::{
    repair::SyncOptions,
    watch::{
        request_watch_control, run_watch_service, WatchConfig, WatchControlRequest,
        WatchControlResponse, WatchServiceMode, WatchServiceStatus,
    },
};

use super::{request_mutation_when_idle, setup_test_repo, wait_for, watch_service_guard};

#[test]
fn watch_service_handles_sync_now_and_set_auto_sync() {
    let _guard = watch_service_guard();
    let (_dir, repo, mut config, synrepo_dir) = setup_test_repo();
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

    let sync_response = request_mutation_when_idle(
        &synrepo_dir,
        WatchControlRequest::SyncNow {
            options: SyncOptions::default(),
        },
    );
    assert!(matches!(sync_response, WatchControlResponse::Sync { .. }));

    for enabled in [false, true] {
        let response =
            request_watch_control(&synrepo_dir, WatchControlRequest::SetAutoSync { enabled })
                .unwrap();
        assert!(matches!(response, WatchControlResponse::Ack { .. }));
    }

    let _ = request_watch_control(&synrepo_dir, WatchControlRequest::Stop);
    handle.join().unwrap();
}
