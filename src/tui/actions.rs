//! Operator-action dispatchers that bridge dashboard quick actions to the
//! existing control-plane primitives in `pipeline::watch::control`, the
//! decomposed `setup::step_*` helpers, and repair surfaces. The dashboard
//! never bypasses the writer lock: every mutating action flows through the
//! same admission path as the equivalent CLI subcommand, so watch-service
//! ownership and writer-lock ownership are both enforced.
//!
//! Actions return a structured [`ActionOutcome`] the dashboard can surface in
//! its bounded log pane. Callers lift the outcome into a [`LogEntry`] via
//! [`outcome_to_log`].

use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use crate::config::Config;
use crate::pipeline::watch::{
    cleanup_stale_watch_artifacts, request_watch_control, run_reconcile_pass, watch_service_status,
    ReconcileOutcome, WatchControlRequest, WatchControlResponse, WatchServiceStatus,
};
use crate::pipeline::writer::{
    acquire_write_admission, current_ownership, live_owner_pid, LockError, WriterOwnershipError,
};
use crate::tui::probe::Severity;
use crate::tui::widgets::LogEntry;

const WATCH_START_TIMEOUT: Duration = Duration::from_secs(2);
const WATCH_STOP_TIMEOUT: Duration = Duration::from_secs(30);

/// Context passed to every action dispatcher. Keeps the callsite in the render
/// loop narrow; callers build this once per app state transition.
#[derive(Clone, Debug)]
pub struct ActionContext {
    /// Repo root (the `--repo` flag value or `cwd`).
    pub repo_root: PathBuf,
    /// Resolved `.synrepo/` directory.
    pub synrepo_dir: PathBuf,
}

impl ActionContext {
    /// Build a context from `repo_root`, deriving the `.synrepo/` path.
    pub fn new(repo_root: &Path) -> Self {
        Self {
            repo_root: repo_root.to_path_buf(),
            synrepo_dir: Config::synrepo_dir(repo_root),
        }
    }
}

/// Structured outcome of a dispatcher call. The dashboard never swallows these
/// — each one is surfaced in the log pane via [`outcome_to_log`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActionOutcome {
    /// Action dispatched and the subsystem acknowledged it (e.g. watch stop).
    Ack {
        /// Short human-readable acknowledgement.
        message: String,
    },
    /// Action completed with an observable result (e.g. reconcile pass).
    Completed {
        /// Short human-readable summary.
        message: String,
    },
    /// Writer-lock or watch-ownership conflict. Structured so the log pane can
    /// name the holder PID and acquisition timestamp without the dashboard
    /// panicking or bypassing the lock.
    Conflict {
        /// PID of the foreign holder, when known.
        owner_pid: Option<u32>,
        /// RFC 3339 timestamp of the lock or lease acquisition, when known.
        acquired_at: Option<String>,
        /// Short description of the conflict surface (`"writer lock"`,
        /// `"watch lease"`, or the like).
        surface: String,
        /// Operator-facing guidance, one line.
        guidance: String,
    },
    /// Action failed with an error not attributable to a lock conflict.
    Error {
        /// Human-readable failure message.
        message: String,
    },
}

/// Run a reconcile pass on behalf of the dashboard. If a watch service owns
/// the repo, delegate via the control socket; otherwise acquire the writer
/// lock and run one pass directly.
///
/// Never bypasses the writer lock. On contention the caller receives an
/// [`ActionOutcome::Conflict`] they can surface in the log pane.
pub fn reconcile_now(ctx: &ActionContext) -> ActionOutcome {
    match watch_service_status(&ctx.synrepo_dir) {
        WatchServiceStatus::Running(state) => {
            match request_watch_control(&ctx.synrepo_dir, WatchControlRequest::ReconcileNow) {
                Ok(WatchControlResponse::Reconcile { outcome, .. }) => {
                    reconcile_outcome_to_action(&outcome)
                }
                Ok(WatchControlResponse::Error { message }) => ActionOutcome::Error { message },
                Ok(_) => ActionOutcome::Error {
                    message: format!(
                        "watch service (pid {}) returned an unexpected response to reconcile-now",
                        state.pid
                    ),
                },
                Err(err) => ActionOutcome::Error {
                    message: format!("reconcile-now delegate failed: {err}"),
                },
            }
        }
        WatchServiceStatus::Starting => ActionOutcome::Conflict {
            owner_pid: None,
            acquired_at: None,
            surface: "watch lease".to_string(),
            guidance:
                "watch service is still starting; wait for it to become ready before reconciling"
                    .to_string(),
        },
        WatchServiceStatus::Inactive => run_local_reconcile(ctx),
        WatchServiceStatus::Stale(_) | WatchServiceStatus::Corrupt(_) => {
            if let Err(err) = cleanup_stale_watch_artifacts(&ctx.synrepo_dir) {
                return ActionOutcome::Error {
                    message: format!("failed to clean stale watch artifacts: {err}"),
                };
            }
            run_local_reconcile(ctx)
        }
    }
}

fn run_local_reconcile(ctx: &ActionContext) -> ActionOutcome {
    let config = match load_repo_config(ctx, "reconcile") {
        Ok(c) => c,
        Err(outcome) => return outcome,
    };

    match acquire_write_admission(&ctx.synrepo_dir, "reconcile") {
        Ok(_lock) => {
            let outcome = run_reconcile_pass(&ctx.repo_root, &config, &ctx.synrepo_dir);
            reconcile_outcome_to_action(&outcome)
        }
        Err(err) => lock_error_to_action(&ctx.synrepo_dir, err),
    }
}

/// Stop the active watch service for this repo. Returns
/// [`ActionOutcome::Ack`] on a successful stop, [`ActionOutcome::Completed`]
/// when artifacts were already stale, or a conflict/error variant otherwise.
pub fn stop_watch(ctx: &ActionContext) -> ActionOutcome {
    match watch_service_status(&ctx.synrepo_dir) {
        WatchServiceStatus::Inactive => ActionOutcome::Completed {
            message: "no active watch service".to_string(),
        },
        WatchServiceStatus::Starting => match wait_for_watch_startup_settled(&ctx.synrepo_dir) {
            Ok(()) => stop_watch(ctx),
            Err(err) => ActionOutcome::Error { message: err },
        },
        WatchServiceStatus::Stale(_) | WatchServiceStatus::Corrupt(_) => {
            match cleanup_stale_watch_artifacts(&ctx.synrepo_dir) {
                Ok(true) => ActionOutcome::Completed {
                    message: "cleaned stale watch artifacts".to_string(),
                },
                Ok(false) => ActionOutcome::Completed {
                    message: "watch service still running; no cleanup performed".to_string(),
                },
                Err(err) => ActionOutcome::Error {
                    message: format!("failed to clean stale watch artifacts: {err}"),
                },
            }
        }
        WatchServiceStatus::Running(state) => {
            match request_watch_control(&ctx.synrepo_dir, WatchControlRequest::Stop) {
                Ok(WatchControlResponse::Ack { message }) => {
                    match wait_for_watch_stopped(&ctx.synrepo_dir) {
                        Ok(()) => ActionOutcome::Ack {
                            message: format!("{message} (pid {})", state.pid),
                        },
                        Err(err) => ActionOutcome::Error { message: err },
                    }
                }
                Ok(WatchControlResponse::Error { message }) => ActionOutcome::Error {
                    message: format!("watch service refused stop: {message}"),
                },
                Ok(_) => ActionOutcome::Error {
                    message: "watch service returned an unexpected response to stop".to_string(),
                },
                Err(err) => recover_stop_transport_error(ctx, err, state.pid),
            }
        }
    }
}

/// Start the watch service in daemon mode by re-exec'ing this binary with
/// `watch --daemon`. Running the service in-process from the dashboard would
/// deadlock the alt-screen; spawning a detached child is the safe analogue of
/// the CLI `synrepo watch --daemon` path the dashboard is replacing.
pub fn start_watch_daemon(ctx: &ActionContext) -> ActionOutcome {
    use std::process::{Command, Stdio};

    if let Err(outcome) = load_repo_config(ctx, "watch") {
        return outcome;
    }

    match watch_service_status(&ctx.synrepo_dir) {
        WatchServiceStatus::Starting => {
            return ActionOutcome::Conflict {
                owner_pid: None,
                acquired_at: None,
                surface: "watch lease".to_string(),
                guidance:
                    "watch service is still starting; wait for it to become ready before starting another".to_string(),
            };
        }
        WatchServiceStatus::Running(state) => {
            return ActionOutcome::Conflict {
                owner_pid: Some(state.pid),
                acquired_at: Some(state.started_at.clone()),
                surface: "watch lease".to_string(),
                guidance: format!(
                    "watch service already running ({}, pid {})",
                    state.mode, state.pid
                ),
            };
        }
        WatchServiceStatus::Stale(_) | WatchServiceStatus::Corrupt(_) => {
            if let Err(err) = cleanup_stale_watch_artifacts(&ctx.synrepo_dir) {
                return ActionOutcome::Error {
                    message: format!("failed to clean stale watch artifacts: {err}"),
                };
            }
        }
        WatchServiceStatus::Inactive => {}
    }

    let exe = match resolve_synrepo_executable() {
        Ok(path) => path,
        Err(message) => {
            return ActionOutcome::Error { message };
        }
    };

    let mut cmd = Command::new(&exe);
    cmd.arg("--repo")
        .arg(&ctx.repo_root)
        .arg("watch")
        .arg("--daemon");
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::null());
    cmd.current_dir(&ctx.repo_root);
    detach_daemon_process(&mut cmd);

    match cmd.spawn() {
        Ok(child) => match wait_for_watch_running(&ctx.synrepo_dir) {
            Ok(()) => ActionOutcome::Ack {
                message: format!("spawned watch daemon (pid {})", child.id()),
            },
            Err(err) => ActionOutcome::Error { message: err },
        },
        Err(err) => ActionOutcome::Error {
            message: format!("failed to spawn watch daemon: {err}"),
        },
    }
}

fn wait_for_watch_running(synrepo_dir: &Path) -> Result<(), String> {
    wait_for_watch_transition(synrepo_dir, WATCH_START_TIMEOUT, |status| match status {
        WatchServiceStatus::Running(_) => Ok(Some(())),
        WatchServiceStatus::Starting => Ok(None),
        WatchServiceStatus::Corrupt(e) => {
            Err(format!("watch state became corrupt during startup: {e}"))
        }
        WatchServiceStatus::Inactive | WatchServiceStatus::Stale(_) => Ok(None),
    })
}

fn wait_for_watch_stopped(synrepo_dir: &Path) -> Result<(), String> {
    wait_for_watch_transition(synrepo_dir, WATCH_STOP_TIMEOUT, |status| match status {
        WatchServiceStatus::Running(_) | WatchServiceStatus::Starting => Ok(None),
        WatchServiceStatus::Inactive => Ok(Some(())),
        WatchServiceStatus::Stale(_) | WatchServiceStatus::Corrupt(_) => {
            cleanup_stale_watch_artifacts(synrepo_dir).map_err(|err| {
                format!("failed to clean stale watch artifacts after stop: {err}")
            })?;
            Ok(Some(()))
        }
    })
}

fn wait_for_watch_startup_settled(synrepo_dir: &Path) -> Result<(), String> {
    wait_for_watch_transition(synrepo_dir, WATCH_START_TIMEOUT, |status| match status {
        WatchServiceStatus::Starting => Ok(None),
        _ => Ok(Some(())),
    })
}

fn wait_for_watch_transition<T>(
    synrepo_dir: &Path,
    timeout: Duration,
    mut check: impl FnMut(WatchServiceStatus) -> Result<Option<T>, String>,
) -> Result<T, String> {
    let deadline = Instant::now() + timeout;
    let mut backoff = Duration::from_millis(10);
    loop {
        match check(watch_service_status(synrepo_dir))? {
            Some(done) => return Ok(done),
            None if Instant::now() >= deadline => {
                return Err(format!(
                    "watch state did not settle within {} ms",
                    timeout.as_millis()
                ));
            }
            None => {
                thread::sleep(backoff);
                backoff = (backoff * 2).min(Duration::from_millis(50));
            }
        }
    }
}

fn recover_stop_transport_error(
    ctx: &ActionContext,
    err: crate::pipeline::watch::WatchDaemonError,
    pid: u32,
) -> ActionOutcome {
    match watch_service_status(&ctx.synrepo_dir) {
        WatchServiceStatus::Inactive => ActionOutcome::Completed {
            message: format!("watch service already stopped (pid {pid})"),
        },
        WatchServiceStatus::Starting => ActionOutcome::Error {
            message: format!("stop request failed while watch service is still starting: {err}"),
        },
        WatchServiceStatus::Stale(_) | WatchServiceStatus::Corrupt(_) => {
            match cleanup_stale_watch_artifacts(&ctx.synrepo_dir) {
                Ok(_) => ActionOutcome::Completed {
                    message: format!("cleaned stale watch artifacts after daemon exit (pid {pid})"),
                },
                Err(cleanup_err) => ActionOutcome::Error {
                    message: format!(
                        "stop request failed: {err}; cleanup also failed: {cleanup_err}"
                    ),
                },
            }
        }
        WatchServiceStatus::Running(_) => ActionOutcome::Error {
            message: format!("stop request failed: {err}"),
        },
    }
}

fn load_repo_config(ctx: &ActionContext, action: &str) -> Result<Config, ActionOutcome> {
    let local_config = ctx.synrepo_dir.join("config.toml");
    if !local_config.exists() {
        return Err(ActionOutcome::Error {
            message: format!("{action}: not initialized — run `synrepo init` first"),
        });
    }
    Config::load(&ctx.repo_root).map_err(|err| ActionOutcome::Error {
        message: format!("{action}: could not load config: {err}"),
    })
}

fn resolve_synrepo_executable() -> Result<PathBuf, String> {
    let current = std::env::current_exe()
        .map_err(|err| format!("could not resolve current executable: {err}"))?;
    let Some(parent) = current.parent() else {
        return Ok(current);
    };
    if parent.file_name().and_then(|name| name.to_str()) == Some("deps") {
        if let Some(target_dir) = parent.parent() {
            let candidate = target_dir.join(format!("synrepo{}", std::env::consts::EXE_SUFFIX));
            if candidate.exists() {
                return Ok(candidate);
            }
        }
    }
    Ok(current)
}

fn detach_daemon_process(command: &mut std::process::Command) {
    #[cfg(unix)]
    {
        command.process_group(0);
    }
}

/// Convert a `ReconcileOutcome` from an in-process or delegated pass into a
/// structured action outcome.
fn reconcile_outcome_to_action(outcome: &ReconcileOutcome) -> ActionOutcome {
    match outcome {
        ReconcileOutcome::Completed(summary) => ActionOutcome::Completed {
            message: format!(
                "reconcile completed ({} files, {} symbols)",
                summary.files_discovered, summary.symbols_extracted
            ),
        },
        ReconcileOutcome::LockConflict { holder_pid } => ActionOutcome::Conflict {
            owner_pid: Some(*holder_pid),
            acquired_at: None,
            surface: "writer lock".to_string(),
            guidance: format!("reconcile skipped: writer lock held by pid {holder_pid}"),
        },
        ReconcileOutcome::Failed(message) => ActionOutcome::Error {
            message: format!("reconcile failed: {message}"),
        },
    }
}

/// Map a `LockError` into a structured action outcome, enriching the lock
/// conflict branch with the ownership metadata pulled from the lock file.
fn lock_error_to_action(synrepo_dir: &Path, err: LockError) -> ActionOutcome {
    match err {
        LockError::HeldByOther { pid, .. } => {
            let acquired_at = match current_ownership(synrepo_dir) {
                Ok(o) => Some(o.acquired_at),
                Err(WriterOwnershipError::NotFound) => None,
                Err(WriterOwnershipError::Malformed(_)) => None,
            };
            ActionOutcome::Conflict {
                owner_pid: Some(pid),
                acquired_at,
                surface: "writer lock".to_string(),
                guidance: format!("writer lock held by pid {pid}; retry when it releases"),
            }
        }
        LockError::WatchOwned { watch_pid } => ActionOutcome::Conflict {
            owner_pid: Some(watch_pid),
            acquired_at: None,
            surface: "watch lease".to_string(),
            guidance: format!(
                "watch service owns this repo (pid {watch_pid}); stop it before mutating"
            ),
        },
        LockError::WatchStarting => ActionOutcome::Conflict {
            owner_pid: None,
            acquired_at: None,
            surface: "watch lease".to_string(),
            guidance:
                "watch service is still starting; wait for it to become ready before mutating"
                    .to_string(),
        },
        LockError::WrongThread { .. } => ActionOutcome::Error {
            message: "writer lock already held by another thread in this process".to_string(),
        },
        LockError::Malformed { lock_path, detail } => ActionOutcome::Error {
            message: format!(
                "writer lock at {} is malformed ({detail})",
                lock_path.display()
            ),
        },
        LockError::Io { path, source } => ActionOutcome::Error {
            message: format!("writer lock I/O error at {}: {source}", path.display()),
        },
    }
}

/// Translate an action outcome into a bounded log-pane entry. Callers append
/// the returned entry to the shared ring buffer so operators see lock
/// conflicts instead of silent failures.
pub fn outcome_to_log(tag: &str, outcome: &ActionOutcome) -> LogEntry {
    let (severity, message) = match outcome {
        ActionOutcome::Ack { message } | ActionOutcome::Completed { message } => {
            (Severity::Healthy, message.clone())
        }
        ActionOutcome::Conflict {
            owner_pid,
            acquired_at,
            surface,
            guidance,
        } => {
            let mut line = format!("{surface} conflict: {guidance}");
            if let Some(pid) = owner_pid {
                line.push_str(&format!(" (owner pid {pid}"));
                if let Some(ts) = acquired_at {
                    line.push_str(&format!(", acquired {ts}"));
                }
                line.push(')');
            }
            (Severity::Stale, line)
        }
        ActionOutcome::Error { message } => (Severity::Blocked, message.clone()),
    };

    LogEntry {
        timestamp: now_rfc3339(),
        tag: tag.to_string(),
        message,
        severity,
    }
}

/// Minimal RFC 3339 stamp without pulling a format dep. Uses `OffsetDateTime`
/// if `time` is already in scope via `surface::status_snapshot`; fallback is
/// epoch seconds so the log pane never loses a timestamp.
pub(crate) fn now_rfc3339() -> String {
    match time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339) {
        Ok(s) => s,
        Err(_) => {
            let secs = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            format!("epoch-{secs}")
        }
    }
}

/// Convenience wrapper: surface the active writer-lock holder, if any, as a
/// structured log entry. Called by the dashboard on startup so the operator
/// sees when another process is mid-write before they try an action.
pub fn writer_lock_hint(ctx: &ActionContext) -> Option<LogEntry> {
    let pid = live_owner_pid(&ctx.synrepo_dir)?;
    let acquired_at = current_ownership(&ctx.synrepo_dir)
        .ok()
        .map(|o| o.acquired_at);
    let outcome = ActionOutcome::Conflict {
        owner_pid: Some(pid),
        acquired_at,
        surface: "writer lock".to_string(),
        guidance: format!("writer lock currently held by pid {pid}"),
    };
    Some(outcome_to_log("lock", &outcome))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn init_repo(path: &Path) {
        crate::bootstrap::bootstrap(path, None, false).expect("bootstrap");
    }

    #[test]
    fn reconcile_now_on_fresh_repo_returns_error() {
        let dir = tempdir().unwrap();
        let ctx = ActionContext::new(dir.path());
        let outcome = reconcile_now(&ctx);
        // No config.toml → load fails with error (not a conflict).
        assert!(
            matches!(outcome, ActionOutcome::Error { .. }),
            "got {outcome:?}"
        );
    }

    #[test]
    fn reconcile_now_on_ready_repo_completes() {
        let _guard = crate::test_support::global_test_lock("tui-reconcile-now");
        let dir = tempdir().unwrap();
        init_repo(dir.path());
        let ctx = ActionContext::new(dir.path());
        let outcome = reconcile_now(&ctx);
        assert!(
            matches!(outcome, ActionOutcome::Completed { .. }),
            "expected Completed, got {outcome:?}"
        );
    }

    #[test]
    fn stop_watch_on_idle_repo_is_completed_noop() {
        let dir = tempdir().unwrap();
        init_repo(dir.path());
        let ctx = ActionContext::new(dir.path());
        let outcome = stop_watch(&ctx);
        assert!(
            matches!(outcome, ActionOutcome::Completed { .. }),
            "got {outcome:?}"
        );
    }

    #[test]
    fn outcome_to_log_conflict_names_owner_pid() {
        let outcome = ActionOutcome::Conflict {
            owner_pid: Some(4242),
            acquired_at: Some("2026-04-17T12:00:00Z".to_string()),
            surface: "writer lock".to_string(),
            guidance: "held".to_string(),
        };
        let entry = outcome_to_log("lock", &outcome);
        assert_eq!(entry.severity, Severity::Stale);
        assert!(entry.message.contains("owner pid 4242"));
        assert!(entry.message.contains("acquired 2026-04-17"));
    }

    // Phase 9.3: lock-contended reconcile-now must not panic and must surface
    // the conflict as a structured outcome. Linux/macOS only — the writer
    // flock helpers are unix-only.
    #[cfg(unix)]
    #[test]
    fn reconcile_now_under_foreign_lock_returns_conflict() {
        use crate::pipeline::writer::{
            hold_writer_flock_with_ownership, writer_lock_path, WriterOwnership,
        };
        use std::fs;

        let _guard = crate::test_support::global_test_lock("tui-reconcile-now");
        let dir = tempdir().unwrap();
        init_repo(dir.path());
        let ctx = ActionContext::new(dir.path());
        let lock_path = writer_lock_path(&ctx.synrepo_dir);
        fs::create_dir_all(lock_path.parent().unwrap()).unwrap();
        let _guard = hold_writer_flock_with_ownership(
            &lock_path,
            &WriterOwnership {
                pid: 99999,
                acquired_at: "2026-04-17T12:00:00Z".to_string(),
            },
        );

        let outcome = reconcile_now(&ctx);
        match outcome {
            ActionOutcome::Conflict {
                owner_pid, surface, ..
            } => {
                assert_eq!(surface, "writer lock");
                assert_eq!(owner_pid, Some(99999));
            }
            other => panic!("expected Conflict, got {other:?}"),
        }
    }

    #[test]
    fn start_watch_daemon_on_ready_repo_acknowledges_and_observes_running() {
        let _guard = crate::test_support::global_test_lock("tui-start-watch-daemon");
        let dir = tempdir().unwrap();
        init_repo(dir.path());
        let ctx = ActionContext::new(dir.path());

        let start = start_watch_daemon(&ctx);
        assert!(matches!(start, ActionOutcome::Ack { .. }), "got {start:?}");
        assert!(
            matches!(
                watch_service_status(&ctx.synrepo_dir),
                WatchServiceStatus::Running(_)
            ),
            "watch should be running after start"
        );

        let stop = stop_watch(&ctx);
        assert!(
            matches!(
                stop,
                ActionOutcome::Ack { .. } | ActionOutcome::Completed { .. }
            ),
            "cleanup stop must succeed, got {stop:?}"
        );
        assert!(
            matches!(
                watch_service_status(&ctx.synrepo_dir),
                WatchServiceStatus::Inactive
            ),
            "watch should be inactive after stop"
        );
    }

    #[test]
    fn start_watch_daemon_reports_conflict_when_already_running() {
        let _guard = crate::test_support::global_test_lock("tui-start-watch-daemon");
        let dir = tempdir().unwrap();
        init_repo(dir.path());
        let ctx = ActionContext::new(dir.path());

        let first = start_watch_daemon(&ctx);
        assert!(matches!(first, ActionOutcome::Ack { .. }), "got {first:?}");
        let second = start_watch_daemon(&ctx);
        assert!(
            matches!(second, ActionOutcome::Conflict { .. }),
            "second start should conflict, got {second:?}"
        );

        let stop = stop_watch(&ctx);
        assert!(
            matches!(
                stop,
                ActionOutcome::Ack { .. } | ActionOutcome::Completed { .. }
            ),
            "cleanup stop must succeed, got {stop:?}"
        );
    }

    #[test]
    fn start_watch_daemon_on_uninitialized_repo_returns_error() {
        let dir = tempdir().unwrap();
        let ctx = ActionContext::new(dir.path());
        let outcome = start_watch_daemon(&ctx);
        assert!(
            matches!(outcome, ActionOutcome::Error { .. }),
            "expected Error, got {outcome:?}"
        );
    }
}
