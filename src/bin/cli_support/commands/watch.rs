use std::{
    path::Path,
    process::{Command, Stdio},
    thread,
    time::Duration,
};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use synrepo::{
    config::Config,
    pipeline::watch::{
        cleanup_stale_watch_artifacts, request_watch_control, run_watch_service,
        watch_service_status, WatchConfig, WatchControlRequest, WatchControlResponse,
        WatchDaemonState, WatchServiceMode, WatchServiceStatus,
    },
};

use super::status::render_watch_summary;

/// Start the watch service in the foreground or daemon mode.
pub(crate) fn watch(repo_root: &Path, daemon: bool) -> anyhow::Result<()> {
    let config = Config::load(repo_root).map_err(|error| {
        anyhow::anyhow!("watch: not initialized — run `synrepo init` first ({error})")
    })?;
    let synrepo_dir = Config::synrepo_dir(repo_root);

    match watch_service_status(&synrepo_dir) {
        WatchServiceStatus::Starting => {
            anyhow::bail!("watch service is still starting; wait for it to become ready");
        }
        WatchServiceStatus::Running(state) => {
            anyhow::bail!(
                "watch service already running in {} mode under pid {}",
                state.mode,
                state.pid
            );
        }
        WatchServiceStatus::Stale(_) => {
            cleanup_stale_watch_artifacts(&synrepo_dir)?;
        }
        WatchServiceStatus::Inactive => {}
        WatchServiceStatus::Corrupt(e) => {
            anyhow::bail!(
                "watch service state is corrupt: {e}. Run `synrepo watch stop` to clean up."
            );
        }
    }

    if daemon {
        let pid = spawn_watch_daemon(repo_root)?;
        println!("Started watch service in daemon mode (pid {pid}).");
        Ok(())
    } else {
        println!(
            "Starting watch service in foreground mode for {}",
            repo_root.display()
        );
        run_watch_service(
            repo_root,
            &config,
            &WatchConfig::default(),
            &synrepo_dir,
            WatchServiceMode::Foreground,
            None,
        )
        .map_err(|error| anyhow::anyhow!(error.to_string()))
    }
}

/// Hidden entrypoint used by `synrepo watch --daemon`.
pub(crate) fn watch_internal(repo_root: &Path) -> anyhow::Result<()> {
    let config = Config::load(repo_root)?;
    let synrepo_dir = Config::synrepo_dir(repo_root);
    run_watch_service(
        repo_root,
        &config,
        &WatchConfig::default(),
        &synrepo_dir,
        WatchServiceMode::Daemon,
        None,
    )
    .map_err(|error| anyhow::anyhow!(error.to_string()))
}

/// Print detailed watch-service status.
pub(crate) fn watch_status(repo_root: &Path) -> anyhow::Result<()> {
    let synrepo_dir = Config::synrepo_dir(repo_root);
    println!("synrepo watch status");

    match watch_service_status(&synrepo_dir) {
        WatchServiceStatus::Inactive => {
            println!("  state:        inactive");
        }
        WatchServiceStatus::Starting => {
            println!("  state:        starting");
            println!("  next step:    wait for the watch service to become ready");
        }
        WatchServiceStatus::Stale(snapshot) => {
            println!("  state:        stale");
            if let Some(snapshot) = snapshot {
                println!("  pid:          {}", snapshot.pid);
                println!("  started:      {}", snapshot.started_at);
            }
            println!("  next step:    run `synrepo watch stop` to clean stale artifacts");
        }
        WatchServiceStatus::Running(snapshot) => {
            let snapshot = match request_watch_control(&synrepo_dir, WatchControlRequest::Status) {
                Ok(WatchControlResponse::Status { snapshot }) => snapshot,
                Ok(_) | Err(_) => snapshot,
            };
            render_watch_status_snapshot(&snapshot);
        }
        WatchServiceStatus::Corrupt(e) => {
            println!("  state:        corrupt ({e})");
            println!("  next step:    run `synrepo watch stop` to clean corrupt artifacts");
        }
    }

    Ok(())
}

/// Stop the active watch service or clean stale watch artifacts.
pub(crate) fn watch_stop(repo_root: &Path) -> anyhow::Result<()> {
    let synrepo_dir = Config::synrepo_dir(repo_root);
    match watch_service_status(&synrepo_dir) {
        WatchServiceStatus::Inactive => {
            println!("No active watch service for this repo.");
            Ok(())
        }
        WatchServiceStatus::Starting => {
            wait_for_watch_startup_settle(&synrepo_dir)?;
            watch_stop(repo_root)
        }
        WatchServiceStatus::Stale(_) => {
            cleanup_stale_watch_artifacts(&synrepo_dir)?;
            println!("Removed stale watch service artifacts.");
            Ok(())
        }
        WatchServiceStatus::Running(state) => {
            match request_watch_control(&synrepo_dir, WatchControlRequest::Stop) {
                Ok(WatchControlResponse::Ack { message }) => {
                    wait_for_watch_shutdown(&synrepo_dir)?;
                    println!("{message}");
                    println!("Stopped watch service (pid {}).", state.pid);
                    Ok(())
                }
                Ok(WatchControlResponse::Status { .. }) => Err(anyhow::anyhow!(
                    "stop request returned a status snapshot instead of an acknowledgement"
                )),
                Ok(WatchControlResponse::Error { message }) => {
                    Err(anyhow::anyhow!("failed to stop watch service: {message}"))
                }
                Ok(WatchControlResponse::Reconcile { .. }) => Err(anyhow::anyhow!(
                    "stop request returned a reconcile response instead of an acknowledgement"
                )),
                Err(err) => recover_stop_transport_error(&synrepo_dir, err, state.pid),
            }
        }
        WatchServiceStatus::Corrupt(e) => {
            cleanup_stale_watch_artifacts(&synrepo_dir)?;
            println!("Removed corrupt watch service artifacts: {e}");
            Ok(())
        }
    }
}

fn recover_stop_transport_error(
    synrepo_dir: &Path,
    err: synrepo::pipeline::watch::WatchDaemonError,
    pid: u32,
) -> anyhow::Result<()> {
    match watch_service_status(synrepo_dir) {
        WatchServiceStatus::Inactive => {
            println!("Watch service already stopped (pid {}).", pid);
            Ok(())
        }
        WatchServiceStatus::Starting => Err(anyhow::anyhow!(
            "stop request failed while watch service is still starting: {err}"
        )),
        WatchServiceStatus::Stale(_) | WatchServiceStatus::Corrupt(_) => {
            cleanup_stale_watch_artifacts(synrepo_dir)?;
            println!(
                "Removed stale watch service artifacts after daemon exit (pid {}).",
                pid
            );
            Ok(())
        }
        WatchServiceStatus::Running(_) => Err(anyhow::anyhow!("stop request failed: {err}")),
    }
}

pub(super) fn ensure_watch_not_running(synrepo_dir: &Path, action: &str) -> anyhow::Result<()> {
    match watch_service_status(synrepo_dir) {
        WatchServiceStatus::Inactive => Ok(()),
        WatchServiceStatus::Starting => Err(anyhow::anyhow!(
            "{action} is unavailable while watch service is still starting. Wait for it to become ready or stop it first."
        )),
        WatchServiceStatus::Stale(_) => {
            cleanup_stale_watch_artifacts(synrepo_dir)?;
            Ok(())
        }
        WatchServiceStatus::Running(state) => Err(anyhow::anyhow!(
            "{action} is unavailable while watch service is active in {} mode (pid {}). Run `synrepo watch stop` first.",
            state.mode,
            state.pid
        )),
        WatchServiceStatus::Corrupt(_) => {
            cleanup_stale_watch_artifacts(synrepo_dir)?;
            Ok(())
        }
    }
}

pub(super) fn active_watch_pid(synrepo_dir: &Path) -> anyhow::Result<Option<u32>> {
    match watch_service_status(synrepo_dir) {
        WatchServiceStatus::Inactive => Ok(None),
        WatchServiceStatus::Starting => Err(anyhow::anyhow!(
            "watch service is still starting; wait for it to become ready before reconciling"
        )),
        WatchServiceStatus::Stale(_) => {
            cleanup_stale_watch_artifacts(synrepo_dir)?;
            Ok(None)
        }
        WatchServiceStatus::Running(state) => Ok(Some(state.pid)),
        WatchServiceStatus::Corrupt(_) => {
            cleanup_stale_watch_artifacts(synrepo_dir)?;
            Ok(None)
        }
    }
}

fn render_watch_status_snapshot(snapshot: &WatchDaemonState) {
    println!("  state:        running");
    println!(
        "  summary:      {}",
        render_watch_summary(&WatchServiceStatus::Running(snapshot.clone()))
    );
    println!("  pid:          {}", snapshot.pid);
    println!("  started:      {}", snapshot.started_at);
    println!("  endpoint:     {}", snapshot.control_endpoint);
    println!(
        "  last event:   {}",
        snapshot
            .last_event_at
            .as_deref()
            .unwrap_or("none observed yet")
    );
    println!(
        "  last run:     {}",
        snapshot
            .last_reconcile_at
            .as_deref()
            .unwrap_or("no reconcile recorded yet")
    );
    println!(
        "  outcome:      {}",
        snapshot
            .last_reconcile_outcome
            .as_deref()
            .unwrap_or("unknown")
    );
    if let Some(error) = &snapshot.last_error {
        println!("  error:        {error}");
    }
}

fn wait_for_watch_startup_settle(synrepo_dir: &Path) -> anyhow::Result<()> {
    for _ in 0..100 {
        if !matches!(
            watch_service_status(synrepo_dir),
            WatchServiceStatus::Starting
        ) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(50));
    }
    anyhow::bail!("watch service did not finish starting in time")
}

fn spawn_watch_daemon(repo_root: &Path) -> anyhow::Result<u32> {
    let exe = std::env::current_exe()
        .map_err(|error| anyhow::anyhow!("could not resolve current executable: {error}"))?;

    // Capture daemon stderr to a file so startup crashes and panics are
    // recoverable post-mortem. File::create truncates on each spawn, bounding
    // the log to the current session without needing rotation. stdout stays
    // nulled so normal tracing output does not grow unbounded.
    let state_dir = Config::synrepo_dir(repo_root).join("state");
    std::fs::create_dir_all(&state_dir).map_err(|error| {
        anyhow::anyhow!(
            "could not create watch state dir {}: {error}",
            state_dir.display()
        )
    })?;
    let log_path = state_dir.join("watch-daemon.log");
    let stderr_file = std::fs::File::create(&log_path).map_err(|error| {
        anyhow::anyhow!(
            "could not open watch daemon log {}: {error}",
            log_path.display()
        )
    })?;

    let mut command = Command::new(&exe);
    command
        .arg("--repo")
        .arg(repo_root)
        .arg("watch-internal")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(stderr_file))
        .current_dir(repo_root);
    detach_daemon_process(&mut command);

    let mut child = command
        .spawn()
        .map_err(|error| anyhow::anyhow!("failed to spawn watch daemon: {error}"))?;

    let synrepo_dir = Config::synrepo_dir(repo_root);
    for _ in 0..100 {
        if let Some(status) = child.try_wait()? {
            anyhow::bail!("watch daemon exited early with status {status}");
        }
        match watch_service_status(&synrepo_dir) {
            WatchServiceStatus::Running(state) if state.pid == child.id() => {
                if matches!(
                    request_watch_control(&synrepo_dir, WatchControlRequest::Status),
                    Ok(WatchControlResponse::Status { .. })
                ) {
                    return Ok(child.id());
                }
            }
            _ => {}
        }
        thread::sleep(Duration::from_millis(50));
    }

    let _ = child.kill();
    anyhow::bail!("watch daemon did not become ready in time")
}

fn detach_daemon_process(command: &mut Command) {
    #[cfg(unix)]
    {
        // Break the daemon out of the launching foreground process group so
        // it survives after `synrepo watch --daemon` or the dashboard
        // process exits. Keeping the child in the caller's process group can
        // leave it vulnerable to terminal hangup and stale leases.
        command.process_group(0);
    }
    #[cfg(not(unix))]
    {
        let _ = command;
    }
}

fn wait_for_watch_shutdown(synrepo_dir: &Path) -> anyhow::Result<()> {
    for _ in 0..600 {
        match watch_service_status(synrepo_dir) {
            WatchServiceStatus::Inactive => return Ok(()),
            WatchServiceStatus::Starting => thread::sleep(Duration::from_millis(50)),
            WatchServiceStatus::Stale(_) => {
                cleanup_stale_watch_artifacts(synrepo_dir)?;
                return Ok(());
            }
            WatchServiceStatus::Running(_) => thread::sleep(Duration::from_millis(50)),
            WatchServiceStatus::Corrupt(_) => {
                cleanup_stale_watch_artifacts(synrepo_dir)?;
                return Ok(());
            }
        }
    }
    anyhow::bail!("watch service did not stop in time")
}
