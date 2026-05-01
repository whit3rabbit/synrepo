#![cfg_attr(not(unix), allow(unused_imports))]

use std::{fs, path::PathBuf};

use super::super::{
    cleanup_stale_watch_artifacts, lease::acquire_watch_daemon_lease, load_watch_state,
    watch_daemon_state_path, watch_service_status, watch_socket_path, WatchDaemonState,
    WatchServiceMode, WatchServiceStatus,
};
#[cfg(unix)]
use super::dead_pid;
use super::setup_test_repo;

// Unix-only: two `acquire_watch_daemon_lease` calls in the same process rely
// on POSIX flock rejecting a second handle. Windows `LockFileEx` is
// per-handle, so the second acquire succeeds in-process. Cross-process
// contention is covered by the soak suite at `tests/mutation_soak.rs`.
#[cfg(unix)]
#[test]
fn watch_lease_blocks_a_second_live_owner() {
    let (_dir, _repo, _config, synrepo_dir) = setup_test_repo();

    let (_lease, _handle) =
        acquire_watch_daemon_lease(&synrepo_dir, WatchServiceMode::Foreground).unwrap();
    let error = acquire_watch_daemon_lease(&synrepo_dir, WatchServiceMode::Daemon).unwrap_err();

    assert!(error.to_string().contains("already running"));
}

#[cfg(unix)]
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
        control_endpoint: socket_path.display().to_string(),
        last_event_at: None,
        last_reconcile_at: None,
        last_reconcile_outcome: None,
        last_error: None,
        last_triggering_events: None,
        auto_sync_enabled: false,
        auto_sync_running: false,
        auto_sync_paused: false,
        auto_sync_last_started_at: None,
        auto_sync_last_finished_at: None,
        auto_sync_last_outcome: None,
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

#[cfg(unix)]
#[test]
fn cleanup_stale_watch_artifacts_removes_dead_state_and_socket() {
    let (_dir, _repo, _config, synrepo_dir) = setup_test_repo();
    let state_path = watch_daemon_state_path(&synrepo_dir);
    let socket_path = watch_socket_path(&synrepo_dir);

    let stale = WatchDaemonState {
        pid: dead_pid(),
        started_at: "2026-04-12T00:00:00Z".to_string(),
        mode: WatchServiceMode::Daemon,
        control_endpoint: socket_path.display().to_string(),
        last_event_at: None,
        last_reconcile_at: None,
        last_reconcile_outcome: None,
        last_error: None,
        last_triggering_events: None,
        auto_sync_enabled: false,
        auto_sync_running: false,
        auto_sync_paused: false,
        auto_sync_last_started_at: None,
        auto_sync_last_finished_at: None,
        auto_sync_last_outcome: None,
    };
    fs::write(&state_path, serde_json::to_string(&stale).unwrap()).unwrap();
    fs::write(&socket_path, b"stale socket").unwrap();

    assert!(cleanup_stale_watch_artifacts(&synrepo_dir).unwrap());
    assert!(load_watch_state(&synrepo_dir).is_err());
    assert!(!socket_path.exists());
}

#[cfg(unix)]
#[test]
fn cleanup_removes_orphan_socket_when_state_file_missing() {
    // Daemon crash after `bind()` but before `watch-daemon.json` was written:
    // the socket is left behind, but no state file exists, so the previous
    // cleanup path short-circuited on Inactive and left the socket orphaned.
    let (_dir, _repo, _config, synrepo_dir) = setup_test_repo();
    let state_path = watch_daemon_state_path(&synrepo_dir);
    let socket_path = watch_socket_path(&synrepo_dir);
    fs::write(&socket_path, b"orphan socket").unwrap();
    assert!(!state_path.exists());
    assert!(socket_path.exists());

    // Status is Inactive (no state file), but cleanup must still sweep the
    // dangling socket so the next `synrepo watch` can bind again.
    assert!(matches!(
        watch_service_status(&synrepo_dir),
        WatchServiceStatus::Inactive
    ));
    assert!(cleanup_stale_watch_artifacts(&synrepo_dir).unwrap());
    assert!(!socket_path.exists());
}

#[test]
fn lock_held_without_state_is_reported_as_starting_and_not_cleaned() {
    use crate::pipeline::watch::lease::watch_flock_path;
    use crate::pipeline::writer::{acquire_write_admission, open_and_try_lock, LockError};

    let (_dir, _repo, _config, synrepo_dir) = setup_test_repo();
    let state_path = watch_daemon_state_path(&synrepo_dir);
    let _flock = open_and_try_lock(&state_path)
        .expect("open+flock must succeed")
        .expect("watch flock must be free");

    assert!(matches!(
        watch_service_status(&synrepo_dir),
        WatchServiceStatus::Starting
    ));
    assert!(!cleanup_stale_watch_artifacts(&synrepo_dir).unwrap());
    assert!(watch_flock_path(&synrepo_dir).exists());
    assert!(matches!(
        acquire_write_admission(&synrepo_dir, "reconcile"),
        Err(LockError::WatchStarting)
    ));
}

#[cfg(unix)]
#[test]
fn corrupt_state_file_is_reported_and_cleaned() {
    // A partially-written or truncated watch-daemon.json must be surfaced as
    // Corrupt (not silently papered over) and must be cleanable by the
    // existing stale-artifact path so operators can recover without rm -rf.
    let (_dir, _repo, _config, synrepo_dir) = setup_test_repo();
    let state_path = watch_daemon_state_path(&synrepo_dir);
    let socket_path = watch_socket_path(&synrepo_dir);
    fs::write(&state_path, b"{ not valid json").unwrap();
    fs::write(&socket_path, b"stale socket").unwrap();

    let status = watch_service_status(&synrepo_dir);
    assert!(
        matches!(status, WatchServiceStatus::Corrupt(_)),
        "expected Corrupt status on garbage state file, got: {status:?}"
    );

    assert!(cleanup_stale_watch_artifacts(&synrepo_dir).unwrap());
    assert!(!state_path.exists());
    assert!(!socket_path.exists());
    assert!(matches!(
        watch_service_status(&synrepo_dir),
        WatchServiceStatus::Inactive
    ));
}

#[test]
fn status_and_reconcile_unaffected_when_daemon_never_registered() {
    // Daemon crash before PID registration: no state file, no socket, nothing
    // to clean up. status must report Inactive; reconcile must proceed.
    use crate::pipeline::watch::{run_reconcile_pass, ReconcileOutcome};
    let (_dir, repo, config, synrepo_dir) = setup_test_repo();
    assert!(matches!(
        watch_service_status(&synrepo_dir),
        WatchServiceStatus::Inactive
    ));
    let outcome = run_reconcile_pass(&repo, &config, &synrepo_dir, false);
    if !matches!(outcome, ReconcileOutcome::Completed(_)) {
        panic!("expected Completed when no daemon was ever registered, got {outcome:?}");
    }
}

#[test]
fn watch_socket_path_fits_sockaddr_un_limit_for_deep_repos() {
    // sockaddr_un.sun_path is 104 bytes on macOS and 108 on Linux
    // (NUL-terminated). Simulate a pathologically deep repo path and
    // confirm the hashed socket path stays well under that ceiling.
    let mut deep = PathBuf::from("/Users/someone.with-a-long-handle/Documents");
    for segment in [
        "clients",
        "acme-enterprise-consulting-services",
        "monorepos",
        "platform-core-shared-infrastructure",
        "services",
        "authentication-and-session-management",
        ".synrepo",
    ] {
        deep.push(segment);
    }

    let socket = watch_socket_path(&deep);
    let len = socket.as_os_str().len();
    assert!(
        len < 100,
        "watch socket path is {} bytes long: {}",
        len,
        socket.display()
    );
}

#[test]
fn watch_socket_path_is_deterministic_for_same_input() {
    let dir = PathBuf::from("/tmp/does/not/exist/synrepo-socket-test/.synrepo");
    assert_eq!(watch_socket_path(&dir), watch_socket_path(&dir));
}

#[test]
fn watch_socket_path_differs_per_repo() {
    let a = PathBuf::from("/tmp/does/not/exist/repo-a/.synrepo");
    let b = PathBuf::from("/tmp/does/not/exist/repo-b/.synrepo");
    assert_ne!(watch_socket_path(&a), watch_socket_path(&b));
}
