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
