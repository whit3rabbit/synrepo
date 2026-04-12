//! Single-writer safety model for `.synrepo/` runtime state.
//!
//! At most one writer may mutate the runtime stores at a time. This module
//! defines the writer lock contract for standalone CLI operation and provides
//! the handoff model for future daemon-assisted mode.
//!
//! ## Single-writer contract
//!
//! 1. Any code path that mutates runtime state (graph, overlay, index) MUST
//!    hold a `WriterLock` for the duration of the write and release it on
//!    completion.
//! 2. If the lock file records a live process PID, `acquire_writer_lock`
//!    returns `Err(LockError::HeldByOther)` and the caller must not proceed.
//! 3. A lock file from a terminated process is treated as stale and silently
//!    replaced on the next acquisition attempt.
//! 4. The lock is released when `WriterLock` is dropped; the Drop impl
//!    removes the lock file.
//!
//! ## Daemon handoff
//!
//! In standalone CLI mode, the CLI process acquires and releases the lock on
//! each write operation. A future daemon-assisted mode would hold the lock for
//! its full lifetime, and CLI processes would read-only or delegate writes to
//! the daemon rather than competing for the lock.

use std::{
    fs,
    io::Write as _,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

/// Ownership record written to `.synrepo/state/writer.lock`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WriterOwnership {
    /// OS process ID of the lock holder.
    pub pid: u32,
    /// RFC 3339 UTC timestamp when the lock was acquired.
    pub acquired_at: String,
}

/// Reason a writer lock acquisition failed.
#[derive(Debug, thiserror::Error)]
pub enum LockError {
    /// Another live process holds the lock.
    #[error("writer lock held by pid {pid}; wait for it to finish or remove {lock_path}")]
    HeldByOther {
        /// PID of the process that currently holds the lock.
        pid: u32,
        /// Path to the lock file.
        lock_path: PathBuf,
    },
    /// I/O error interacting with the lock file.
    #[error("I/O error acquiring writer lock at {path}: {source}")]
    Io {
        /// Path where the I/O error occurred.
        path: PathBuf,
        #[source]
        /// Underlying I/O error.
        source: std::io::Error,
    },
}

/// RAII writer lock. Holds the write token for `.synrepo/`.
///
/// Released (lock file removed) when this guard is dropped. Callers must
/// not write to any runtime store without holding this guard.
#[derive(Debug)]
pub struct WriterLock {
    path: PathBuf,
}

impl WriterLock {
    /// Path to the lock file held by this guard.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for WriterLock {
    fn drop(&mut self) {
        if let Err(e) = fs::remove_file(&self.path) {
            tracing::warn!(path = ?self.path, error = %e, "failed to remove writer lock file on drop");
        }
    }
}

/// Try to acquire the exclusive writer lock for `.synrepo/`.
///
/// Returns `Ok(WriterLock)` on success; the lock is released when the guard
/// is dropped. Returns `Err(LockError::HeldByOther)` when another live
/// process holds the lock.
///
/// Uses `O_CREAT|O_EXCL` semantics (`create_new`) for an atomic exclusive
/// create: the OS guarantees only one caller wins the race, so two concurrent
/// invocations cannot both believe they hold the lock.
///
/// A stale lock file from a terminated process is removed and the acquire
/// is retried once without error.
pub fn acquire_writer_lock(synrepo_dir: &Path) -> Result<WriterLock, LockError> {
    let lock_path = writer_lock_path(synrepo_dir);
    let state_dir = synrepo_dir.join("state");

    fs::create_dir_all(&state_dir).map_err(|source| LockError::Io {
        path: state_dir.clone(),
        source,
    })?;

    let ownership = WriterOwnership {
        pid: std::process::id(),
        acquired_at: now_rfc3339(),
    };
    let json = serde_json::to_string(&ownership).expect("WriterOwnership serializes without error");

    // Retry once after clearing a stale lock; two iterations are sufficient
    // because the only reason to loop is a single stale-lock removal.
    let mut cleared_stale = false;
    loop {
        match fs::OpenOptions::new()
            .create_new(true) // O_CREAT|O_EXCL — atomic; returns AlreadyExists if file is present
            .write(true)
            .open(&lock_path)
        {
            Ok(mut f) => {
                f.write_all(json.as_bytes())
                    .map_err(|source| LockError::Io {
                        path: lock_path.clone(),
                        source,
                    })?;
                return Ok(WriterLock { path: lock_path });
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                if cleared_stale {
                    // Already cleared a stale lock once; another live process
                    // won the race on the retry. Report current holder.
                    return Err(match read_ownership(&lock_path) {
                        Ok(owner) => LockError::HeldByOther {
                            pid: owner.pid,
                            lock_path: lock_path.clone(),
                        },
                        Err(_) => LockError::Io {
                            path: lock_path.clone(),
                            source: e,
                        },
                    });
                }
                // First AlreadyExists: check whether the recorded PID is alive.
                match read_ownership(&lock_path) {
                    Ok(owner) if is_process_alive(owner.pid) => {
                        return Err(LockError::HeldByOther {
                            pid: owner.pid,
                            lock_path: lock_path.clone(),
                        });
                    }
                    _ => {
                        // Dead PID or unreadable (malformed/truncated): clear and retry.
                        let _ = fs::remove_file(&lock_path);
                        cleared_stale = true;
                    }
                }
            }
            Err(source) => {
                return Err(LockError::Io {
                    path: lock_path.clone(),
                    source,
                });
            }
        }
    }
}

/// Read current writer ownership from the lock file, if present and readable.
///
/// Returns `None` when no lock file exists or the file cannot be parsed.
/// Does not check whether the recorded PID is still alive.
pub fn current_ownership(synrepo_dir: &Path) -> Option<WriterOwnership> {
    read_ownership(&writer_lock_path(synrepo_dir)).ok()
}

/// Canonical path of the writer lock file within `.synrepo/state/`.
pub fn writer_lock_path(synrepo_dir: &Path) -> PathBuf {
    synrepo_dir.join("state").join("writer.lock")
}

fn read_ownership(lock_path: &Path) -> Result<WriterOwnership, ()> {
    let text = fs::read_to_string(lock_path).map_err(|_| ())?;
    serde_json::from_str(&text).map_err(|_| ())
}

/// Check whether a process is alive using `kill -0 <pid>` on Unix.
///
/// On non-Unix platforms, conservatively returns `true` (assumes alive) to
/// prevent spurious stale-lock takeover on untested operating systems.
fn is_process_alive(pid: u32) -> bool {
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

pub(super) fn now_rfc3339() -> String {
    use time::format_description::well_known::Rfc3339;
    time::OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string())
}

/// Spawn a short-lived child process and wait for it to exit, returning
/// its (now-dead) PID. Used in tests to obtain a reliably dead PID.
#[cfg(test)]
fn spawn_and_reap_pid() -> u32 {
    let mut child = std::process::Command::new("true")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn true");
    let pid = child.id();
    child.wait().expect("wait for child");
    pid
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn acquire_and_drop_removes_lock_file() {
        let dir = tempdir().unwrap();
        let synrepo_dir = dir.path().join(".synrepo");

        let lock = acquire_writer_lock(&synrepo_dir).unwrap();
        assert!(lock.path().exists(), "lock file must exist while held");

        drop(lock);
        assert!(
            !writer_lock_path(&synrepo_dir).exists(),
            "lock file must be removed on drop"
        );
    }

    #[test]
    fn lock_file_records_current_pid() {
        let dir = tempdir().unwrap();
        let synrepo_dir = dir.path().join(".synrepo");

        let _lock = acquire_writer_lock(&synrepo_dir).unwrap();
        let owner = current_ownership(&synrepo_dir).unwrap();
        assert_eq!(owner.pid, std::process::id());
    }

    #[test]
    fn current_ownership_is_none_after_lock_dropped() {
        let dir = tempdir().unwrap();
        let synrepo_dir = dir.path().join(".synrepo");

        {
            let _lock = acquire_writer_lock(&synrepo_dir).unwrap();
        }
        assert!(current_ownership(&synrepo_dir).is_none());
    }

    #[test]
    fn second_acquire_fails_while_first_lock_is_live() {
        let dir = tempdir().unwrap();
        let synrepo_dir = dir.path().join(".synrepo");

        let _lock = acquire_writer_lock(&synrepo_dir).unwrap();
        // Our own process is alive, so a second acquire in the same process
        // must fail with HeldByOther.
        let result = acquire_writer_lock(&synrepo_dir);
        assert!(
            matches!(result, Err(LockError::HeldByOther { .. })),
            "expected HeldByOther, got {result:?}",
        );
    }

    #[test]
    fn stale_lock_from_dead_pid_is_replaced() {
        let dir = tempdir().unwrap();
        let synrepo_dir = dir.path().join(".synrepo");
        std::fs::create_dir_all(synrepo_dir.join("state")).unwrap();

        let dead = spawn_and_reap_pid();

        let stale = WriterOwnership {
            pid: dead,
            acquired_at: "2000-01-01T00:00:00Z".to_string(),
        };
        std::fs::write(
            writer_lock_path(&synrepo_dir),
            serde_json::to_string_pretty(&stale).unwrap(),
        )
        .unwrap();

        let lock = acquire_writer_lock(&synrepo_dir).unwrap();
        let owner = current_ownership(&synrepo_dir).unwrap();
        assert_eq!(
            owner.pid,
            std::process::id(),
            "stale lock must be replaced with current PID"
        );
        drop(lock);
    }

    #[test]
    fn malformed_lock_file_is_replaced() {
        let dir = tempdir().unwrap();
        let synrepo_dir = dir.path().join(".synrepo");
        std::fs::create_dir_all(synrepo_dir.join("state")).unwrap();

        std::fs::write(writer_lock_path(&synrepo_dir), b"not valid json").unwrap();

        let lock = acquire_writer_lock(&synrepo_dir).unwrap();
        assert!(lock.path().exists());
        drop(lock);
    }

    #[test]
    fn concurrent_acquire_only_one_succeeds() {
        // Both threads race to acquire the lock on the same dir.
        // Exactly one must succeed; the other must get HeldByOther.
        use std::sync::Arc;

        let dir = tempdir().unwrap();
        let synrepo_dir = Arc::new(dir.path().join(".synrepo"));

        let dir1 = Arc::clone(&synrepo_dir);
        let dir2 = Arc::clone(&synrepo_dir);

        let h1 = std::thread::spawn(move || acquire_writer_lock(&dir1));
        let h2 = std::thread::spawn(move || acquire_writer_lock(&dir2));

        let r1 = h1.join().unwrap();
        let r2 = h2.join().unwrap();

        let successes = [&r1, &r2].iter().filter(|r| r.is_ok()).count();
        let conflicts = [&r1, &r2]
            .iter()
            .filter(|r| matches!(r, Err(LockError::HeldByOther { .. })))
            .count();

        assert_eq!(successes, 1, "exactly one thread must acquire the lock");
        assert_eq!(conflicts, 1, "exactly one thread must see HeldByOther");
    }
}
