//! Materializer supervisor for the TUI dashboard.
//!
//! Owns a single optional background thread running [`bootstrap`]. Lets the
//! dashboard recover from `graph: not materialized` without forcing the
//! operator out to a CLI, and gives the `M` key a non-blocking dispatch
//! path. Modeled after [`crate::tui::watcher::WatcherSupervisor`].
//!
//! The supervisor never bypasses the writer lock: `bootstrap()` acquires it
//! internally. The dashboard performs an upstream watch-status check in
//! `actions::materialize_now` before kicking off work; the supervisor itself
//! still tolerates a race because `bootstrap()` rejects writer-lock
//! contention with a structured error that we surface as `Failed`.

use std::path::{Path, PathBuf};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use crossbeam_channel::{bounded, Receiver, TryRecvError};

use crate::bootstrap::bootstrap;
use crate::store::sqlite::SqliteGraphStore;

/// Lifecycle view-state the dashboard reads when rendering the health row
/// and footer hint.
#[derive(Clone, Debug)]
pub enum MaterializeState {
    /// No materialization in flight; no auto attempt has been observed yet.
    Idle,
    /// Background thread running. `started_at` drives the
    /// `materializing... (Ns)` row label.
    Running {
        /// Wall-clock start of the in-flight bootstrap.
        started_at: Instant,
    },
    /// Most recent attempt completed cleanly.
    Completed {
        /// Files and symbols recorded by the post-bootstrap snapshot.
        summary: String,
        /// Wall-clock completion time.
        finished_at: Instant,
    },
    /// Most recent attempt failed (lock contention, compile error, ...).
    Failed {
        /// One-line operator-facing error.
        error: String,
        /// Wall-clock completion time.
        finished_at: Instant,
    },
}

impl MaterializeState {
    /// True while a background thread is running.
    pub fn is_running(&self) -> bool {
        matches!(self, MaterializeState::Running { .. })
    }
}

/// Outcome posted by the supervisor thread on completion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MaterializeOutcome {
    /// Bootstrap returned cleanly. Counts come from a fresh
    /// `persisted_stats()` read on the new graph store.
    Completed {
        /// Persisted file-node count.
        files: usize,
        /// Persisted symbol-node count.
        symbols: usize,
    },
    /// Bootstrap returned an error. The string is rendered with `{:#}` so the
    /// full error chain is visible in the log pane.
    Failed {
        /// Operator-facing error message.
        error: String,
    },
}

/// Returned by [`MaterializerSupervisor::start`] when a thread is already in
/// flight; the dashboard surfaces this as an `Ack` toast rather than a hard
/// error.
#[derive(Debug)]
pub struct AlreadyRunning;

/// Supervisor owning an optional background thread that calls `bootstrap()`.
///
/// Construction is cheap: only stores the repo root. The thread is spawned
/// lazily by [`start`](Self::start) and reaped by [`try_drain`](Self::try_drain).
pub struct MaterializerSupervisor {
    repo_root: PathBuf,
    state: MaterializeState,
    rx: Option<Receiver<MaterializeOutcome>>,
    thread: Option<JoinHandle<()>>,
    attempted_auto: bool,
}

impl MaterializerSupervisor {
    /// Build a new supervisor for `repo_root`.
    pub fn new(repo_root: &Path) -> Self {
        Self {
            repo_root: repo_root.to_path_buf(),
            state: MaterializeState::Idle,
            rx: None,
            thread: None,
            attempted_auto: false,
        }
    }

    /// Snapshot of the current lifecycle state.
    pub fn state(&self) -> &MaterializeState {
        &self.state
    }

    /// True while a background bootstrap is in flight.
    pub fn is_running(&self) -> bool {
        self.state.is_running()
    }

    /// True once the dashboard has fired its one-shot auto attempt this
    /// session, regardless of outcome. Manual `M` presses do not flip this
    /// flag, so a manual retry is always available.
    pub fn auto_was_attempted(&self) -> bool {
        self.attempted_auto
    }

    /// Mark the one-shot auto attempt as having fired this session. Called by
    /// the dashboard tick so subsequent ticks observing a missing graph do
    /// not spam bootstrap.
    pub fn mark_auto_attempted(&mut self) {
        self.attempted_auto = true;
    }

    /// Spawn a thread running `bootstrap()` and transition to
    /// `Running`. Returns [`AlreadyRunning`] if a thread is already in
    /// flight; the caller surfaces that as an `Ack` so a double press on
    /// `M` is harmless.
    pub fn start(&mut self) -> Result<(), AlreadyRunning> {
        if self.is_running() {
            return Err(AlreadyRunning);
        }
        // Reap any prior thread before respawning.
        self.join_finished_thread();

        let (tx, rx) = bounded::<MaterializeOutcome>(1);
        let repo_root = self.repo_root.clone();
        let handle = thread::spawn(move || {
            let outcome = run_bootstrap(&repo_root);
            // Receiver may have been dropped if the dashboard closed mid-run;
            // the send error is intentionally swallowed.
            let _ = tx.send(outcome);
        });

        self.rx = Some(rx);
        self.thread = Some(handle);
        self.state = MaterializeState::Running {
            started_at: Instant::now(),
        };
        Ok(())
    }

    /// Non-blocking poll. Returns the outcome on the tick where the
    /// background thread completes; subsequent polls return `None` until the
    /// next `start()`. Transitions the lifecycle state to `Completed` or
    /// `Failed` for the dashboard to render.
    pub fn try_drain(&mut self) -> Option<MaterializeOutcome> {
        let rx = self.rx.as_ref()?;
        let outcome = match rx.try_recv() {
            Ok(outcome) => outcome,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => MaterializeOutcome::Failed {
                error: "bootstrap thread exited before reporting".to_string(),
            },
        };
        self.rx = None;
        self.join_finished_thread();
        self.state = match &outcome {
            MaterializeOutcome::Completed { files, symbols } => MaterializeState::Completed {
                summary: format!("{files} files, {symbols} symbols"),
                finished_at: Instant::now(),
            },
            MaterializeOutcome::Failed { error } => MaterializeState::Failed {
                error: error.clone(),
                finished_at: Instant::now(),
            },
        };
        Some(outcome)
    }

    fn join_finished_thread(&mut self) {
        if let Some(handle) = self.thread.take() {
            // Best effort: the thread always exits cleanly because the work
            // happens inside `run_bootstrap` and the channel send is
            // infallible from the supervisor's perspective.
            let _ = handle.join();
        }
    }
}

impl Drop for MaterializerSupervisor {
    fn drop(&mut self) {
        // Detach the thread on drop. `bootstrap()` is bounded by the work it
        // does and exits on its own; we do not synchronously wait for it
        // here because the dashboard may be unwinding under a panic.
        let _ = self.thread.take();
    }
}

/// Run `bootstrap()` and translate the result into a `MaterializeOutcome`.
/// Lives at module scope so it is unit-testable independently of the thread
/// machinery.
fn run_bootstrap(repo_root: &Path) -> MaterializeOutcome {
    match bootstrap(repo_root, None, false) {
        Ok(_report) => match read_graph_stats(repo_root) {
            Ok((files, symbols)) => MaterializeOutcome::Completed { files, symbols },
            // Bootstrap succeeded but we could not re-open the graph to count
            // nodes; report 0/0 rather than fail since the graph IS there.
            Err(_) => MaterializeOutcome::Completed {
                files: 0,
                symbols: 0,
            },
        },
        Err(err) => MaterializeOutcome::Failed {
            error: format!("{err:#}"),
        },
    }
}

fn read_graph_stats(repo_root: &Path) -> anyhow::Result<(usize, usize)> {
    let synrepo_dir = crate::config::Config::synrepo_dir(repo_root);
    let graph = SqliteGraphStore::open_existing(&synrepo_dir.join("graph"))?;
    let stats = graph.persisted_stats()?;
    Ok((stats.file_nodes, stats.symbol_nodes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn new_supervisor_is_idle_with_no_auto_attempt() {
        let dir = tempdir().unwrap();
        let sup = MaterializerSupervisor::new(dir.path());
        assert!(!sup.is_running());
        assert!(!sup.auto_was_attempted());
        assert!(matches!(sup.state(), MaterializeState::Idle));
    }

    #[test]
    fn mark_auto_attempted_flips_flag() {
        let dir = tempdir().unwrap();
        let mut sup = MaterializerSupervisor::new(dir.path());
        sup.mark_auto_attempted();
        assert!(sup.auto_was_attempted());
    }

    #[test]
    fn start_then_drain_completes_on_fresh_repo() {
        let _guard = crate::test_support::global_test_lock("tui-materializer");
        let home = tempdir().unwrap();
        let _home_guard = crate::config::test_home::HomeEnvGuard::redirect_to(home.path());
        let dir = tempdir().unwrap();
        let mut sup = MaterializerSupervisor::new(dir.path());
        sup.start().expect("start");
        assert!(sup.is_running());

        // Spin until the supervisor reports completion. Bootstrap on an
        // empty tempdir is fast (no source files to compile).
        let deadline = Instant::now() + std::time::Duration::from_secs(30);
        let outcome = loop {
            if let Some(o) = sup.try_drain() {
                break o;
            }
            if Instant::now() >= deadline {
                panic!("materializer did not finish within 30s");
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        assert!(
            matches!(outcome, MaterializeOutcome::Completed { .. }),
            "expected Completed, got {outcome:?}"
        );
        assert!(matches!(sup.state(), MaterializeState::Completed { .. }));
        // Graph file must exist on disk.
        let db_path = dir.path().join(".synrepo/graph/nodes.db");
        assert!(db_path.exists(), "graph db not written: {db_path:?}");
    }

    #[test]
    fn double_start_returns_already_running() {
        let _guard = crate::test_support::global_test_lock("tui-materializer");
        let home = tempdir().unwrap();
        let _home_guard = crate::config::test_home::HomeEnvGuard::redirect_to(home.path());
        let dir = tempdir().unwrap();
        let mut sup = MaterializerSupervisor::new(dir.path());
        sup.start().expect("first start");
        let second = sup.start();
        assert!(second.is_err(), "second start must yield AlreadyRunning");
        // Drain so the test does not leak a running thread into the next test.
        let deadline = Instant::now() + std::time::Duration::from_secs(30);
        while sup.try_drain().is_none() {
            if Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }
}
