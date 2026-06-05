#![cfg(unix)]

use std::{thread, time::Duration};

use crate::pipeline::watch::{
    request_watch_control, run_watch_service, WatchConfig, WatchControlRequest,
    WatchControlResponse, WatchServiceMode, WatchServiceStatus,
};

use super::{setup_test_repo, wait_for, watch_service_guard};

#[test]
fn active_watch_accepts_embedding_build_request() {
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

    let response = request_watch_control(&synrepo_dir, WatchControlRequest::EmbeddingsBuildNow)
        .expect("watch should answer embedding build requests");
    match response {
        WatchControlResponse::EmbeddingsBuild { .. } => {}
        WatchControlResponse::Error { message } => {
            assert!(
                message.contains("embeddings"),
                "error should come from embedding build path, got: {message}"
            );
        }
        other => panic!("unexpected control response: {other:?}"),
    }

    let _ = request_watch_control(&synrepo_dir, WatchControlRequest::Stop);
    handle.join().unwrap();
}

#[cfg(feature = "semantic-triage")]
#[test]
fn delegated_reconcile_marks_existing_embedding_index_stale() {
    let _guard = watch_service_guard();
    let (_dir, repo, mut config, synrepo_dir) = setup_test_repo();
    config.enable_semantic_triage = true;
    config.auto_sync_enabled = false;
    write_placeholder_embedding_index(&synrepo_dir);

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

    let startup_status = request_watch_status(&synrepo_dir);
    assert!(
        !startup_status.embedding_index_stale,
        "startup reconcile must not mark embeddings stale"
    );

    let response = request_watch_control(
        &synrepo_dir,
        WatchControlRequest::ReconcileNow { fast: true },
    )
    .expect("watch should answer reconcile requests");
    assert!(
        matches!(response, WatchControlResponse::Reconcile { .. }),
        "expected reconcile response, got {response:?}"
    );

    let after = request_watch_status(&synrepo_dir);
    assert!(
        after.embedding_index_stale,
        "delegated reconcile should schedule existing embedding index refresh"
    );

    let _ = request_watch_control(&synrepo_dir, WatchControlRequest::Stop);
    handle.join().unwrap();
}

#[cfg(feature = "semantic-triage")]
fn write_placeholder_embedding_index(synrepo_dir: &std::path::Path) {
    let index = synrepo_dir.join("index/vectors/index.bin");
    std::fs::create_dir_all(index.parent().unwrap()).unwrap();
    std::fs::write(index, b"placeholder").unwrap();
}

#[cfg(feature = "semantic-triage")]
fn request_watch_status(synrepo_dir: &std::path::Path) -> crate::pipeline::watch::WatchDaemonState {
    match request_watch_control(synrepo_dir, WatchControlRequest::Status).expect("watch status") {
        WatchControlResponse::Status { snapshot } => snapshot,
        other => panic!("expected status response, got {other:?}"),
    }
}
