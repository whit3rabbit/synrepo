//! Internal helpers for the writer lock implementation.
//!
//! Contains re-entrancy tracking, file I/O, process liveness checks, and
//! timestamp formatting. These are implementation details not exposed in the
//! public API.

use std::{
    collections::HashMap,
    fs,
    io::Write as _,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    thread::ThreadId,
};

use super::{LockError, WriterOwnership, WriterOwnershipError};

// ---- Re-entrancy tracking ----

/// Per-lock-path re-entrancy state: depth counter plus the owning thread.
static LOCK_DEPTHS: OnceLock<Mutex<HashMap<PathBuf, ReentrancyState>>> = OnceLock::new();

struct ReentrancyState {
    depth: usize,
    owner_thread: ThreadId,
}

fn lock_depths() -> &'static Mutex<HashMap<PathBuf, ReentrancyState>> {
    LOCK_DEPTHS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Increment depth for `lock_path` on the current thread.
/// Returns `Err(LockError::WrongThread)` if a different thread already holds
/// the lock for this path.
pub(super) fn try_increment_depth(lock_path: &Path) -> Result<(), LockError> {
    let mut map = lock_depths().lock().unwrap();
    let current = std::thread::current().id();
    match map.get(lock_path) {
        Some(existing) if existing.owner_thread != current => {
            return Err(LockError::WrongThread {
                lock_path: lock_path.to_path_buf(),
            });
        }
        _ => {}
    }
    map.entry(lock_path.to_path_buf())
        .and_modify(|e| e.depth += 1)
        .or_insert(ReentrancyState {
            depth: 1,
            owner_thread: current,
        });
    Ok(())
}

/// Decrement depth for `lock_path` and return the value *after* decrement.
pub(super) fn decrement_depth(lock_path: &Path) -> usize {
    let mut map = lock_depths().lock().unwrap();
    let Some(entry) = map.get_mut(lock_path) else {
        return 0;
    };
    entry.depth = entry.depth.saturating_sub(1);
    let remaining = entry.depth;
    if remaining == 0 {
        map.remove(lock_path);
    }
    remaining
}

// ---- File I/O helpers ----

pub(super) fn read_ownership(lock_path: &Path) -> Result<WriterOwnership, WriterOwnershipError> {
    let text = fs::read_to_string(lock_path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            WriterOwnershipError::NotFound
        } else {
            WriterOwnershipError::Malformed(e.to_string())
        }
    })?;
    serde_json::from_str(&text).map_err(|e| WriterOwnershipError::Malformed(e.to_string()))
}

/// Read ownership, retrying briefly on parse failure to ride out the narrow
/// race between a concurrent `create_new` and its subsequent `write_all`.
pub(super) fn read_ownership_with_retry(
    lock_path: &Path,
) -> Result<WriterOwnership, WriterOwnershipError> {
    const RETRIES: u32 = 5;
    const BACKOFF_MS: u64 = 1;
    for _ in 0..RETRIES {
        if let Ok(owner) = read_ownership(lock_path) {
            return Ok(owner);
        }
        std::thread::sleep(std::time::Duration::from_millis(BACKOFF_MS));
    }
    read_ownership(lock_path)
}

pub(super) fn write_lock_file(
    file: &mut fs::File,
    lock_path: &Path,
    json: &str,
) -> Result<(), LockError> {
    if let Err(source) = file.write_all(json.as_bytes()) {
        cleanup_partial_lock_file(lock_path);
        return Err(LockError::Io {
            path: lock_path.to_path_buf(),
            source,
        });
    }

    if let Err(source) = file.sync_all() {
        cleanup_partial_lock_file(lock_path);
        return Err(LockError::Io {
            path: lock_path.to_path_buf(),
            source,
        });
    }

    Ok(())
}

fn cleanup_partial_lock_file(lock_path: &Path) {
    if let Err(error) = fs::remove_file(lock_path) {
        if error.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(
                path = ?lock_path,
                error = %error,
                "failed to clean up partially written writer lock file"
            );
        }
    }
}

// ---- Process liveness ----

/// Check whether a process is alive using `kill -0 <pid>` on Unix.
///
/// On non-Unix platforms, conservatively returns `true` (assumes alive) to
/// prevent spurious stale-lock takeover on untested operating systems.
pub(crate) fn is_process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        true
    }
}

/// Current UTC time as RFC 3339 string.
pub fn now_rfc3339() -> String {
    use time::format_description::well_known::Rfc3339;
    time::OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string())
}

/// Spawn a short-lived child process and wait for it to exit, returning
/// its (now-dead) PID. Used in tests to obtain a reliably dead PID.
#[cfg(all(test, unix))]
pub(in crate::pipeline::writer) fn spawn_and_reap_pid() -> u32 {
    let mut child = std::process::Command::new("true")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn true");
    let pid = child.id();
    child.wait().expect("wait for child");
    pid
}
