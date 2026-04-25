// All helpers and tests in this file use the watch daemon, which requires Unix
// sockets and is only supported on unix platforms.
#![cfg_attr(not(unix), allow(dead_code, unused_imports))]

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

use tempfile::TempDir;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_synrepo")
}

fn command(repo: &Path) -> Command {
    let mut command = Command::new(bin());
    command.arg("--repo").arg(repo);
    command
}

fn run_ok(repo: &Path, args: &[&str]) -> String {
    let output = command(repo).args(args).output().unwrap();
    assert_success(output)
}

fn assert_success(output: Output) -> String {
    assert!(
        output.status.success(),
        "command failed: stdout={}, stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn wait_for_output(repo: &Path, args: &[&str], needle: &str) {
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut last_status = None;
    let mut last_stdout = String::new();
    let mut last_stderr = String::new();
    while Instant::now() < deadline {
        let output = command(repo).args(args).output().unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        if output.status.success() && stdout.contains(needle) {
            return;
        }
        last_status = output.status.code();
        last_stdout = stdout.to_string();
        last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
        thread::sleep(Duration::from_millis(50));
    }
    panic!(
        "timed out waiting for {:?}; last_status={:?}; stdout={}; stderr={}",
        args, last_status, last_stdout, last_stderr
    );
}

fn init_repo() -> TempDir {
    let repo = tempfile::tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(repo.path().join("src/lib.rs"), "pub fn hello() {}\n").unwrap();
    run_ok(repo.path(), &["init"]);
    repo
}

struct WatchGuard {
    repo: PathBuf,
    child: Option<Child>,
}

impl WatchGuard {
    fn daemon(repo: &Path) -> Self {
        run_ok(repo, &["watch", "--daemon"]);
        wait_for_output(repo, &["watch", "status"], "state:        running");
        Self {
            repo: repo.to_path_buf(),
            child: None,
        }
    }

    fn foreground(repo: &Path) -> Self {
        let child = command(repo)
            .arg("watch")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        wait_for_output(repo, &["watch", "status"], "summary:      foreground mode");
        Self {
            repo: repo.to_path_buf(),
            child: Some(child),
        }
    }
}

impl Drop for WatchGuard {
    fn drop(&mut self) {
        let _ = command(&self.repo).args(["watch", "stop"]).output();
        if let Some(child) = &mut self.child {
            let _ = child.wait();
        }
    }
}

#[cfg(unix)]
#[test]
fn foreground_watch_reports_status_and_stops_cleanly() {
    let repo = init_repo();
    let mut guard = WatchGuard::foreground(repo.path());

    let status = run_ok(repo.path(), &["watch", "status"]);
    assert!(status.contains("summary:      foreground mode"));

    let stop = run_ok(repo.path(), &["watch", "stop"]);
    assert!(stop.contains("Stopped watch service"));

    if let Some(child) = &mut guard.child {
        let status = child.wait().unwrap();
        assert!(status.success());
    }
}

#[cfg(unix)]
#[test]
fn daemon_watch_delegates_reconcile_and_surfaces_status() {
    let repo = init_repo();
    let _guard = WatchGuard::daemon(repo.path());

    fs::write(repo.path().join("src/new.rs"), "pub fn new_fn() {}\n").unwrap();
    let reconcile = run_ok(repo.path(), &["reconcile"]);
    assert!(reconcile.contains("Delegated reconcile to active watch service"));

    let status = run_ok(repo.path(), &["status"]);
    assert!(status.contains("watch:        daemon mode"));
}

#[cfg(unix)]
#[test]
fn sync_delegates_to_watch_service_when_active() {
    // Regression guard: `synrepo sync` used to fail fast with "watch service
    // is active" while the daemon held the lease. As of the
    // sync-watch-delegation-v1 change, it delegates over the control socket
    // and returns the same SyncSummary the watch service produced.
    let repo = init_repo();
    let _guard = WatchGuard::daemon(repo.path());

    let output = command(repo.path()).args(["sync"]).output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "sync should succeed via delegation; stderr={stderr} stdout={stdout}"
    );
    assert!(
        stderr.contains("Delegated sync to active watch service"),
        "expected delegation banner on stderr; stderr={stderr}"
    );
    assert!(
        stdout.contains("sync:"),
        "expected sync summary on stdout; stdout={stdout}"
    );
}
