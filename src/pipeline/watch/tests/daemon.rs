use std::fs;

use super::super::{
    cleanup_stale_watch_artifacts, lease::acquire_watch_daemon_lease, load_watch_state,
    watch_daemon_state_path, watch_service_status, watch_socket_path, WatchDaemonState,
    WatchServiceMode, WatchServiceStatus,
};
use super::{dead_pid, setup_test_repo};

#[test]
fn watch_lease_blocks_a_second_live_owner() {
    let (_dir, _repo, _config, synrepo_dir) = setup_test_repo();

    let (_lease, _handle) =
        acquire_watch_daemon_lease(&synrepo_dir, WatchServiceMode::Foreground).unwrap();
    let error = acquire_watch_daemon_lease(&synrepo_dir, WatchServiceMode::Daemon).unwrap_err();

    assert!(error.to_string().contains("already running"));
}

#[test]
fn watch_lease_replaces_stale_owner() {
    let (_dir, _repo, _config, synrepo_dir) = setup_test_repo();
    let state_path = watch_daemon_state_path(&synrepo_dir);
    let socket_path = watch_socket_path(&synrepo_dir);
    fs::write(&socket_path, b"stale socket").unwrap();

    let stale = WatchDaemonState {
        pid: dead_pid(),
        started_at: "2026-04-12T00:00:00Z".to_string(),
        mode: WatchServiceMode::Daemon,
        socket_path: socket_path.display().to_string(),
        last_event_at: None,
        last_reconcile_at: None,
        last_reconcile_outcome: None,
        last_error: None,
        last_triggering_events: None,
    };
    fs::write(&state_path, serde_json::to_string(&stale).unwrap()).unwrap();

    let (_lease, handle) =
        acquire_watch_daemon_lease(&synrepo_dir, WatchServiceMode::Foreground).unwrap();
    let state = handle.snapshot();
    assert_eq!(state.mode, WatchServiceMode::Foreground);
    assert_eq!(
        watch_service_status(&synrepo_dir),
        WatchServiceStatus::Running(state)
    );
}

#[test]
fn cleanup_stale_watch_artifacts_removes_dead_state_and_socket() {
    let (_dir, _repo, _config, synrepo_dir) = setup_test_repo();
    let state_path = watch_daemon_state_path(&synrepo_dir);
    let socket_path = watch_socket_path(&synrepo_dir);

    let stale = WatchDaemonState {
        pid: dead_pid(),
        started_at: "2026-04-12T00:00:00Z".to_string(),
        mode: WatchServiceMode::Daemon,
        socket_path: socket_path.display().to_string(),
        last_event_at: None,
        last_reconcile_at: None,
        last_reconcile_outcome: None,
        last_error: None,
        last_triggering_events: None,
    };
    fs::write(&state_path, serde_json::to_string(&stale).unwrap()).unwrap();
    fs::write(&socket_path, b"stale socket").unwrap();

    assert!(cleanup_stale_watch_artifacts(&synrepo_dir).unwrap());
    assert!(load_watch_state(&synrepo_dir).is_none());
    assert!(!socket_path.exists());
}
