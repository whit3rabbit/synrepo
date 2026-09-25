use std::path::Path;

#[cfg(unix)]
use super::{cleanup_stale_watch_artifacts, watch_service_status, WatchServiceStatus};
use super::{WatchDaemonState, WatchServiceMode};

/// Recover a daemon whose control listener is unreachable while its lease is held.
/// Only the detached daemon is eligible for a process signal.
pub fn recover_unreachable_watch(
    synrepo_dir: &Path,
    state: &WatchDaemonState,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        state.mode == WatchServiceMode::Daemon,
        "foreground watch has no reachable control socket; stop its terminal process"
    );
    #[cfg(unix)]
    {
        verify_daemon_owner(synrepo_dir, state)?;
        // Recheck the lease after the comparatively slow process inspection.
        anyhow::ensure!(
            matches!(watch_service_status(synrepo_dir), WatchServiceStatus::Running(ref current) if current.same_owner(state)),
            "watch ownership changed during stop recovery"
        );
        verify_daemon_owner(synrepo_dir, state)?;
        let result = std::process::Command::new("kill")
            .args(["-TERM", &state.pid.to_string()])
            .output()?;
        anyhow::ensure!(
            result.status.success(),
            "could not signal verified watch daemon {}",
            state.pid
        );
        for _ in 0..600 {
            match watch_service_status(synrepo_dir) {
                WatchServiceStatus::Running(ref current) if current.same_owner(state) => {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                WatchServiceStatus::Running(_) => {
                    anyhow::bail!("another watch service acquired the lease")
                }
                WatchServiceStatus::Starting => {
                    anyhow::bail!("watch ownership changed during shutdown")
                }
                WatchServiceStatus::Inactive | WatchServiceStatus::Stale(_) => {
                    cleanup_stale_watch_artifacts(synrepo_dir)?;
                    return Ok(());
                }
                WatchServiceStatus::Corrupt(_) => {
                    anyhow::bail!("watch state became unreadable during shutdown")
                }
            }
        }
        anyhow::bail!("verified watch daemon did not release its lease within 30 seconds")
    }
    #[cfg(not(unix))]
    {
        let _ = synrepo_dir;
        anyhow::bail!(
            "watch control socket is unreachable; process fallback is unavailable on this platform"
        )
    }
}

#[cfg(unix)]
fn verify_daemon_owner(synrepo_dir: &Path, state: &WatchDaemonState) -> anyhow::Result<()> {
    use time::format_description::well_known::Rfc3339;

    let pid = state.pid.to_string();
    let owner = process_output("ps", &["-p", &pid, "-o", "uid="])?;
    let uid = process_output("id", &["-u"])?;
    anyhow::ensure!(
        owner.trim() == uid.trim(),
        "watch daemon is not owned by this user"
    );

    let args = process_output("ps", &["-p", &pid, "-o", "args="])?;
    let args = args.trim();
    anyhow::ensure!(
        args.ends_with(" watch-internal") && args.contains(" --repo "),
        "pid {} is not a synrepo watch daemon",
        state.pid
    );
    let executable = args.split(" --repo ").next().unwrap_or_default();
    let exe_path = std::path::Path::new(executable);
    anyhow::ensure!(
        exe_path.file_name().is_some_and(|name| name == "synrepo"),
        "pid {} has an unexpected executable",
        state.pid
    );
    let running_exe = std::fs::canonicalize(exe_path)?;
    let current_exe = std::env::current_exe()?.canonicalize()?;
    let on_path = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|dir| dir.join("synrepo"))
        .filter_map(|path| path.canonicalize().ok())
        .any(|path| path == running_exe);
    anyhow::ensure!(
        running_exe == current_exe || on_path,
        "pid {} uses a different synrepo executable",
        state.pid
    );

    let started = time::OffsetDateTime::parse(&state.started_at, &Rfc3339)?;
    let expected_elapsed = (time::OffsetDateTime::now_utc() - started).whole_seconds();
    let elapsed = process_output("ps", &["-p", &pid, "-o", "etime="])?;
    let elapsed = parse_elapsed(elapsed.trim())?;
    anyhow::ensure!(
        (expected_elapsed - elapsed).abs() <= 5,
        "pid {} start time does not match the watch lease",
        state.pid
    );

    let flock = super::lease::watch_flock_path(synrepo_dir);
    let holders = process_output("lsof", &["-t", "--", &flock.to_string_lossy()])?;
    anyhow::ensure!(
        holders.lines().any(|line| line.trim() == pid),
        "pid {} does not hold this watch lease",
        state.pid
    );
    Ok(())
}

#[cfg(unix)]
fn process_output(program: &str, args: &[&str]) -> anyhow::Result<String> {
    let output = std::process::Command::new(program).args(args).output()?;
    anyhow::ensure!(
        output.status.success(),
        "{program} failed while verifying watch daemon"
    );
    Ok(String::from_utf8(output.stdout)?)
}

#[cfg(unix)]
fn parse_elapsed(text: &str) -> anyhow::Result<i64> {
    let (days, clock) = match text.split_once('-') {
        Some((days, clock)) => (days.parse::<i64>()?, clock),
        None => (0, text),
    };
    let parts: Vec<i64> = clock.split(':').map(str::parse).collect::<Result<_, _>>()?;
    let seconds = match parts.as_slice() {
        [minutes, seconds] => minutes * 60 + seconds,
        [hours, minutes, seconds] => hours * 3600 + minutes * 60 + seconds,
        _ => anyhow::bail!("unexpected process elapsed time: {text}"),
    };
    Ok(days * 86400 + seconds)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn refuses_to_signal_non_daemon_lease_holder() {
        let repo = tempfile::tempdir().unwrap();
        let synrepo_dir = repo.path().join(".synrepo");
        let state = WatchDaemonState::new(&synrepo_dir, WatchServiceMode::Daemon);
        let _holder = super::super::hold_watch_flock_with_state(&synrepo_dir, &state);
        let error = recover_unreachable_watch(&synrepo_dir, &state)
            .unwrap_err()
            .to_string();
        assert!(error.contains("not a synrepo watch daemon"), "{error}");
    }

    #[test]
    fn elapsed_time_parser_accepts_days_and_hours() {
        assert_eq!(parse_elapsed("06-21:27:36").unwrap(), 595_656);
        assert_eq!(parse_elapsed("03:02").unwrap(), 182);
    }
}
