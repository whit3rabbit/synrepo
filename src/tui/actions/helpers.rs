use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::config::Config;
use crate::pipeline::writer::{current_ownership, live_owner_pid, LockError, WriterOwnershipError};
use crate::tui::probe::Severity;
use crate::tui::widgets::LogEntry;

use super::ActionOutcome;
use super::{ActionContext, ProjectActionContext};

pub(super) fn load_repo_config(ctx: &ActionContext, action: &str) -> Result<Config, ActionOutcome> {
    let local_config = ctx.synrepo_dir.join("config.toml");
    if !local_config.exists() {
        return Err(ActionOutcome::Error {
            message: format!("{action}: not initialized, run `synrepo init` first"),
        });
    }
    Config::load(&ctx.repo_root).map_err(|err| ActionOutcome::Error {
        message: format!("{action}: could not load config: {err}"),
    })
}

#[cfg_attr(all(test, windows), allow(dead_code))]
pub(super) fn resolve_synrepo_executable() -> Result<PathBuf, String> {
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

#[cfg_attr(all(test, windows), allow(dead_code))]
pub(super) fn detach_daemon_process(command: &mut std::process::Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = command;
    }
}

/// Map a `LockError` into a structured action outcome, enriching the lock
/// conflict branch with the ownership metadata pulled from the lock file.
pub(super) fn lock_error_to_action(synrepo_dir: &Path, err: LockError) -> ActionOutcome {
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

/// Translate an action outcome into a log entry labelled with a project name.
pub fn outcome_to_project_log(
    ctx: &ProjectActionContext,
    tag: &str,
    outcome: &ActionOutcome,
) -> LogEntry {
    let mut entry = outcome_to_log(tag, outcome);
    entry.message = format!("[{}] {}", ctx.project_name, entry.message);
    entry
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
