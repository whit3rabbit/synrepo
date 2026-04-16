#[cfg(unix)]
use std::process::Command;
use std::{
    fs,
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

use tempfile::{tempdir, TempDir};

use crate::{config::Config, store::compatibility::write_runtime_snapshot};

mod daemon;
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
pub(super) fn dead_pid() -> u32 {
    let mut child = Command::new("true")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn true");
    let pid = child.id();
    child.wait().expect("wait for true");
    pid
}

#[cfg(unix)]
pub(super) fn live_foreign_pid() -> (std::process::Child, u32) {
    let child = Command::new("sleep")
        .arg("10")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn sleep");
    let pid = child.id();
    (child, pid)
}
