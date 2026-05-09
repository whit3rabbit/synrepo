use std::{
    fs,
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

#[cfg(unix)]
use std::sync::{Mutex, MutexGuard};

use tempfile::{tempdir, TempDir};

#[cfg(unix)]
pub(super) use crate::pipeline::writer::{live_foreign_pid, spawn_and_reap_pid as dead_pid};
use crate::{config::Config, store::compatibility::write_runtime_snapshot};

mod auto_sync;
mod daemon;
mod filter;
mod keepalive;
mod lease;
mod overflow;
mod reconcile;
mod service;

pub(super) fn setup_test_repo() -> (TempDir, PathBuf, Config, PathBuf) {
    let dir = tempdir().unwrap();
    let repo = dir.path().to_path_buf();
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(repo.join("src/lib.rs"), "pub fn hello() {}\n").unwrap();
    let synrepo_dir = repo.join(".synrepo");
    fs::create_dir_all(synrepo_dir.join("state")).unwrap();
    write_runtime_snapshot(&synrepo_dir, &Config::default()).unwrap();
    (dir, repo, Config::default(), synrepo_dir)
}

pub(super) fn wait_for(mut predicate: impl FnMut() -> bool, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if predicate() {
            return;
        }
        thread::sleep(Duration::from_millis(25));
    }
    panic!("condition was not met within {:?}", timeout);
}

#[cfg(unix)]
static WATCH_SERVICE_TEST_LOCK: Mutex<()> = Mutex::new(());

#[cfg(unix)]
pub(super) fn watch_service_guard() -> (
    MutexGuard<'static, ()>,
    crate::test_support::GlobalTestLock,
    crate::test_support::GlobalTestLock,
    MutexGuard<'static, ()>,
) {
    // Order: in-process Mutex (watch-service exclusivity) → cross-process
    // flock for "watch-service" → cross-process flock for HOME → in-process
    // HOME mutex. The HOME flock keeps cross-binary tests from racing; the
    // HOME mutex serializes against `HomeEnvGuard::redirect_to` callers in
    // *this* process (those don't take the flock). `user_socket_dir` reads
    // `$HOME`, so any concurrent mutator could shift the control socket
    // directory mid-test. HOME-mutating tests never take "watch-service",
    // so no AB/BA cycle is possible.
    (
        WATCH_SERVICE_TEST_LOCK
            .lock()
            .expect("watch service test lock poisoned"),
        crate::test_support::global_test_lock("watch-service"),
        crate::test_support::global_test_lock(crate::config::test_home::HOME_ENV_TEST_LOCK),
        crate::config::test_home::lock_home_env_read(),
    )
}
