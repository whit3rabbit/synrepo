use super::*;
use crate::pipeline::watch::{hold_watch_flock_with_state, WatchDaemonState, WatchServiceMode};
use std::fs;

fn make_repo() -> (
    tempfile::TempDir,
    tempfile::TempDir,
    crate::config::test_home::HomeEnvGuard,
) {
    let home = tempfile::tempdir().unwrap();
    let home_guard = crate::config::test_home::HomeEnvGuard::redirect_to(home.path());
    let tempdir = tempfile::tempdir().unwrap();
    let synrepo_dir = tempdir.path().join(".synrepo");
    fs::create_dir_all(synrepo_dir.join("state")).unwrap();
    fs::write(
        synrepo_dir.join("config.toml"),
        "mode = \"auto\"\nroots = [\".\"]\n",
    )
    .unwrap();
    (tempdir, home, home_guard)
}

#[test]
fn probe_returns_off_when_no_lease() {
    let (repo, _home, _home_guard) = make_repo();
    let mut sup = WatcherSupervisor::new(repo.path()).unwrap();
    assert_eq!(sup.probe(), WatcherMode::Off);
    assert_eq!(sup.mode(), WatcherMode::Off);
}

#[test]
fn probe_returns_external_when_flocked_lease_is_held() {
    let (repo, _home, _home_guard) = make_repo();
    let synrepo_dir = repo.path().join(".synrepo");
    let mut state = WatchDaemonState::new(&synrepo_dir, WatchServiceMode::Daemon);
    state.pid = 999_999;
    state.started_at = "2026-04-18T00:00:00Z".to_string();
    state.control_endpoint = "/tmp/synrepo-fake.sock".to_string();
    let _holder = hold_watch_flock_with_state(&synrepo_dir, &state);

    let mut sup = WatcherSupervisor::new(repo.path()).unwrap();
    assert_eq!(sup.probe(), WatcherMode::External { pid: 999_999 });
}

#[test]
fn mark_thread_exited_resets_owned_to_off() {
    let (repo, _home, _home_guard) = make_repo();
    let mut sup = WatcherSupervisor::new(repo.path()).unwrap();
    sup.mode = WatcherMode::OwnedRunning;
    sup.mark_thread_exited();
    assert_eq!(sup.mode(), WatcherMode::Off);
}

#[test]
fn mark_thread_exited_leaves_external_untouched() {
    let (repo, _home, _home_guard) = make_repo();
    let mut sup = WatcherSupervisor::new(repo.path()).unwrap();
    sup.mode = WatcherMode::External { pid: 42 };
    sup.mark_thread_exited();
    assert_eq!(sup.mode(), WatcherMode::External { pid: 42 });
}

#[test]
fn wait_for_service_ready_times_out_without_binding() {
    let (repo, _home, _home_guard) = make_repo();
    let (_done_tx, done_rx) = mpsc::channel::<anyhow::Result<()>>();
    let started = Instant::now();
    let err = wait_for_service_ready(
        &repo.path().join(".synrepo"),
        Duration::from_millis(100),
        &done_rx,
    )
    .unwrap_err();
    assert!(matches!(err, WatcherError::StartTimeout { .. }));
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "startup timeout path should return promptly"
    );
}

#[test]
fn service_join_is_bounded_when_worker_is_still_busy() {
    let handle = thread::spawn(|| thread::sleep(Duration::from_millis(250)));
    let started = Instant::now();
    assert!(!join_service_thread_bounded(
        handle,
        Duration::from_millis(20)
    ));
    assert!(started.elapsed() < Duration::from_millis(150));
}

#[test]
fn service_join_reaps_completed_worker() {
    let handle = thread::spawn(|| {});
    assert!(join_service_thread_bounded(handle, Duration::from_secs(1)));
}
