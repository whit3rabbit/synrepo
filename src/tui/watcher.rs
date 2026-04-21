//! Watcher supervisor for the TUI dashboard.
//!
//! Owns the lifecycle of an in-process `run_watch_service` background thread
//! so both the `synrepo watch` foreground entry point and the bare-`synrepo`
//! dashboard can auto-start a watcher, and so the dashboard's `w` quick
//! action can toggle it at runtime.
//!
//! When the repo's lease is already owned by an external daemon we do not
//! start a competing service; the supervisor tracks `External { pid }` and
//! the toggle action becomes a no-op. This is deliberate: killing a foreign
//! daemon from the dashboard would surprise unrelated consumers.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;

use crate::config::Config;
use crate::pipeline::watch::{
    request_watch_control, run_watch_service, watch_service_status, WatchConfig,
    WatchControlRequest, WatchControlResponse, WatchEvent, WatchServiceMode, WatchServiceStatus,
};

/// Current supervisor-tracked watcher state. This is a view model over the
/// underlying lease file: `probe()` refreshes it from disk, `start()` /
/// `stop()` transition it in-process.
#[allow(dead_code)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WatcherMode {
    /// No watcher is running and the supervisor has not started one.
    Off,
    /// The supervisor has started a watcher on a background thread.
    OwnedRunning,
    /// A foreign process (another `synrepo watch` invocation) owns the lease.
    /// The supervisor will not start or stop this service.
    External {
        /// PID of the external owner, for display in the log pane.
        pid: u32,
    },
}

/// Errors produced while starting or stopping the supervised watcher.
#[derive(Debug, thiserror::Error)]
pub enum WatcherError {
    /// The supervisor tried to start but the service thread never reported
    /// ready through the control plane within the startup deadline.
    #[error("watch service failed to come up within {timeout_ms} ms")]
    StartTimeout {
        /// Timeout that elapsed, for logging.
        timeout_ms: u64,
    },
    /// The service thread exited before reporting ready. Carries the error
    /// message the thread posted on its `done` channel so the operator sees
    /// the real cause (e.g. lease contention with a foreign daemon that
    /// raced us between `probe()` and `start()`).
    #[error("watch service exited during startup: {message}")]
    StartupExited {
        /// Error chain reported by `run_watch_service`.
        message: String,
    },
    /// `start()` was called while a service thread was already running. This
    /// is a state-machine bug in the caller; the supervisor refuses to spawn
    /// a competing thread rather than leak the original.
    #[error("watch service is already running")]
    AlreadyRunning,
}

/// Probe the control plane until it answers `Status` or the deadline expires.
/// Probes first so a fast service (connect success in < 1 ms) is not gated
/// on a fixed pre-probe sleep; on miss, sleeps with exponential backoff
/// starting at 5 ms and capped at 50 ms. `done_rx` surfaces the real startup
/// error when the service thread exits before binding the control endpoint
/// (typical when a foreign daemon raced us for the flock).
fn wait_for_service_ready(
    synrepo_dir: &Path,
    timeout: Duration,
    done_rx: &mpsc::Receiver<anyhow::Result<()>>,
) -> Result<(), WatcherError> {
    let deadline = Instant::now() + timeout;
    let mut backoff = Duration::from_millis(5);
    loop {
        if matches!(
            request_watch_control(synrepo_dir, WatchControlRequest::Status),
            Ok(WatchControlResponse::Status { .. })
        ) {
            return Ok(());
        }
        if let Ok(result) = done_rx.try_recv() {
            let message = match result {
                Ok(()) => "service thread exited cleanly before binding control plane".to_string(),
                Err(err) => format!("{err:#}"),
            };
            return Err(WatcherError::StartupExited { message });
        }
        if Instant::now() >= deadline {
            return Err(WatcherError::StartTimeout {
                timeout_ms: timeout.as_millis() as u64,
            });
        }
        thread::sleep(backoff);
        backoff = (backoff * 2).min(Duration::from_millis(50));
    }
}

/// Supervisor owning an optional in-process watch service thread.
///
/// Construction is cheap: it only records paths and loads the current
/// `Config`. The service is spawned lazily in `start()` and torn down in
/// `stop()` or on drop.
pub struct WatcherSupervisor {
    repo_root: PathBuf,
    synrepo_dir: PathBuf,
    config: Config,
    /// Handle to the background thread hosting `run_watch_service`. `Some`
    /// while `OwnedRunning`.
    service_thread: Option<JoinHandle<()>>,
    /// Thread-exit signalling channel. The receiver is drained on `stop()`
    /// to collect any late-reported error, but we do not currently surface
    /// that error back to the caller (the dashboard log already records the
    /// transition).
    done_rx: Option<mpsc::Receiver<anyhow::Result<()>>>,
    mode: WatcherMode,
}

impl WatcherSupervisor {
    /// Build a new supervisor for `repo_root`. Loads `Config` eagerly so the
    /// dashboard fails fast on malformed configs before entering the alt
    /// screen.
    pub fn new(repo_root: &Path) -> anyhow::Result<Self> {
        let synrepo_dir = Config::synrepo_dir(repo_root);
        let config = Config::load(repo_root)?;
        Ok(Self {
            repo_root: repo_root.to_path_buf(),
            synrepo_dir,
            config,
            service_thread: None,
            done_rx: None,
            mode: WatcherMode::Off,
        })
    }

    /// Test-only constructor that skips disk IO and pins the reported mode.
    /// Used by dashboard tests that exercise the `w` quick-action label in
    /// each `WatcherMode` without spinning up a real config or watch
    /// service. Calling `start()` / `stop()` on an instance built this way
    /// is undefined; the test surface is label rendering only.
    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn mock_for_test(mode: WatcherMode) -> Self {
        Self {
            repo_root: PathBuf::new(),
            synrepo_dir: PathBuf::new(),
            config: Config::default(),
            service_thread: None,
            done_rx: None,
            mode,
        }
    }

    /// Refresh `mode` from the on-disk lease. Call on dashboard open and
    /// after manual intervention. Does not mutate the watcher itself.
    #[allow(dead_code)]
    pub fn probe(&mut self) -> WatcherMode {
        let next = match watch_service_status(&self.synrepo_dir) {
            WatchServiceStatus::Running(state) => WatcherMode::External { pid: state.pid },
            WatchServiceStatus::Starting
            | WatchServiceStatus::Inactive
            | WatchServiceStatus::Stale(_)
            | WatchServiceStatus::Corrupt(_) => {
                if self.service_thread.is_some() {
                    WatcherMode::OwnedRunning
                } else {
                    WatcherMode::Off
                }
            }
        };
        self.mode = next.clone();
        next
    }

    /// Current tracked mode without touching disk.
    #[allow(dead_code)]
    pub fn mode(&self) -> WatcherMode {
        self.mode.clone()
    }

    /// Spawn the watch service on a background thread and return its event
    /// receiver. Blocks until the control plane answers or the timeout
    /// elapses.
    ///
    /// On timeout, sends a best-effort `Stop` to the partially-started
    /// service before joining so a slow `notify`-backed startup cannot wedge
    /// the dashboard thread in `join()` forever.
    pub fn start(&mut self) -> Result<Receiver<WatchEvent>, WatcherError> {
        if self.service_thread.is_some() {
            return Err(WatcherError::AlreadyRunning);
        }

        let (event_tx, event_rx) = crossbeam_channel::bounded::<WatchEvent>(256);
        let (done_tx, done_rx) = mpsc::channel::<anyhow::Result<()>>();

        let service_repo = self.repo_root.clone();
        let service_config = self.config.clone();
        let service_synrepo_dir = self.synrepo_dir.clone();
        let handle = thread::spawn(move || {
            let result = run_watch_service(
                &service_repo,
                &service_config,
                &WatchConfig::default(),
                &service_synrepo_dir,
                WatchServiceMode::Foreground,
                Some(event_tx),
            )
            .map_err(|e| anyhow::anyhow!(e.to_string()));
            let _ = done_tx.send(result);
        });

        self.service_thread = Some(handle);
        self.done_rx = Some(done_rx);

        let done_rx_ref = self
            .done_rx
            .as_ref()
            .expect("done_rx was just populated above");
        match wait_for_service_ready(&self.synrepo_dir, Duration::from_secs(2), done_rx_ref) {
            Ok(()) => {
                self.mode = WatcherMode::OwnedRunning;
                Ok(event_rx)
            }
            Err(err) => {
                // Tell the partially-started service to exit so `join()`
                // below cannot block forever on a thread we cannot signal.
                let _ = request_watch_control(&self.synrepo_dir, WatchControlRequest::Stop);
                if let Some(t) = self.service_thread.take() {
                    let _ = t.join();
                }
                self.done_rx = None;
                Err(err)
            }
        }
    }

    /// Send `Stop` to the running service, join the thread, and clear the
    /// receiver. No-op when the supervisor is `Off` or `External`.
    pub fn stop(&mut self) {
        let Some(handle) = self.service_thread.take() else {
            return;
        };
        if let Err(err) = request_watch_control(&self.synrepo_dir, WatchControlRequest::Stop) {
            tracing::warn!(error = %err, "failed to send stop to TUI-hosted watch service");
        }
        let _ = handle.join();
        if let Some(rx) = self.done_rx.take() {
            while rx.try_recv().is_ok() {}
        }
        self.mode = WatcherMode::Off;
    }

    /// Clear an `OwnedRunning` mode when the event channel has observed
    /// `Disconnected`, i.e. the service thread exited on its own. The
    /// dashboard's `drain_events` path wires this up so the quick-actions
    /// label and future `w` toggle reflect reality.
    #[allow(dead_code)]
    pub fn mark_thread_exited(&mut self) {
        if self.service_thread.is_some() {
            let _ = self.service_thread.take();
        }
        self.done_rx = None;
        if matches!(self.mode, WatcherMode::OwnedRunning) {
            self.mode = WatcherMode::Off;
        }
    }
}

impl Drop for WatcherSupervisor {
    fn drop(&mut self) {
        // Best-effort cleanup so a panicking dashboard does not leak a lease.
        if self.service_thread.is_some() {
            self.stop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::watch::{hold_watch_flock_with_state, WatchDaemonState, WatchServiceMode};
    use std::fs;

    fn make_repo() -> tempfile::TempDir {
        let tempdir = tempfile::tempdir().unwrap();
        let synrepo_dir = tempdir.path().join(".synrepo");
        fs::create_dir_all(synrepo_dir.join("state")).unwrap();
        fs::write(
            synrepo_dir.join("config.toml"),
            "mode = \"auto\"\nroots = [\".\"]\n",
        )
        .unwrap();
        tempdir
    }

    #[test]
    fn probe_returns_off_when_no_lease() {
        let repo = make_repo();
        let mut sup = WatcherSupervisor::new(repo.path()).unwrap();
        assert_eq!(sup.probe(), WatcherMode::Off);
        assert_eq!(sup.mode(), WatcherMode::Off);
    }

    #[test]
    fn probe_returns_external_when_flocked_lease_is_held() {
        let repo = make_repo();
        let synrepo_dir = repo.path().join(".synrepo");
        // Simulate a foreign live daemon: write the state file AND hold
        // the kernel flock on a separate fd so `watch_service_status`
        // reports `Running`.
        let state = WatchDaemonState {
            pid: 999_999,
            started_at: "2026-04-18T00:00:00Z".to_string(),
            mode: WatchServiceMode::Daemon,
            control_endpoint: "/tmp/synrepo-fake.sock".to_string(),
            last_event_at: None,
            last_reconcile_at: None,
            last_reconcile_outcome: None,
            last_error: None,
            last_triggering_events: None,
        };
        let _holder = hold_watch_flock_with_state(&synrepo_dir, &state);

        let mut sup = WatcherSupervisor::new(repo.path()).unwrap();
        assert_eq!(sup.probe(), WatcherMode::External { pid: 999_999 });
    }

    #[test]
    fn mark_thread_exited_resets_owned_to_off() {
        let repo = make_repo();
        let mut sup = WatcherSupervisor::new(repo.path()).unwrap();
        sup.mode = WatcherMode::OwnedRunning;
        sup.mark_thread_exited();
        assert_eq!(sup.mode(), WatcherMode::Off);
    }

    #[test]
    fn mark_thread_exited_leaves_external_untouched() {
        let repo = make_repo();
        let mut sup = WatcherSupervisor::new(repo.path()).unwrap();
        sup.mode = WatcherMode::External { pid: 42 };
        sup.mark_thread_exited();
        assert_eq!(sup.mode(), WatcherMode::External { pid: 42 });
    }

    #[test]
    fn wait_for_service_ready_times_out_without_binding() {
        let repo = make_repo();
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
}
