use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use crate::pipeline::watch::{
    cleanup_stale_watch_artifacts, control_endpoint_reachable, request_watch_control,
    watch_service_status, WatchControlRequest, WatchControlResponse, WatchServiceStatus,
};

use super::helpers::{detach_daemon_process, load_repo_config, resolve_synrepo_executable};
use super::{ActionContext, ActionOutcome};

const WATCH_START_TIMEOUT: Duration = Duration::from_secs(2);
const WATCH_STOP_TIMEOUT: Duration = Duration::from_secs(30);

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
    // `Running` says the daemon holds its lease and has written state, but
    // the listener thread may not have bound the control socket yet. Poll for
    // endpoint reachability so callers never observe the `Running`-but-ENOENT
    // race window that used to turn `stop_watch` into a spurious error.
    wait_for_watch_transition(synrepo_dir, WATCH_START_TIMEOUT, |status| match status {
        WatchServiceStatus::Running(_) if control_endpoint_reachable(synrepo_dir) => Ok(Some(())),
        WatchServiceStatus::Running(_) | WatchServiceStatus::Starting => Ok(None),
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

pub(super) fn recover_stop_transport_error(
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
