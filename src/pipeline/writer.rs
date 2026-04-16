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
    collections::HashMap,
    fs,
    io::Write as _,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

use serde::{Deserialize, Serialize};

/// Per-lock-path re-entrancy depth counters.
///
/// Uses an absolute canonical path as key so that different relative path
/// spellings for the same file do not create independent entries. A global
/// `AtomicUsize` would interfere across concurrent tests that use distinct
/// `synrepo_dir` paths, so we track depth per path.
static LOCK_DEPTHS: OnceLock<Mutex<HashMap<PathBuf, usize>>> = OnceLock::new();

fn lock_depths() -> &'static Mutex<HashMap<PathBuf, usize>> {
    LOCK_DEPTHS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn increment_depth(lock_path: &Path) {
    let mut map = lock_depths().lock().unwrap();
    *map.entry(lock_path.to_path_buf()).or_insert(0) += 1;
}

/// Decrement depth for `lock_path` and return the value *after* decrement.
fn decrement_depth(lock_path: &Path) -> usize {
    let mut map = lock_depths().lock().unwrap();
    let entry = map.entry(lock_path.to_path_buf()).or_insert(0);
    *entry = entry.saturating_sub(1);
    let remaining = *entry;
    if remaining == 0 {
        map.remove(lock_path);
    }
    remaining
}

/// Ownership record written to `.synrepo/state/writer.lock`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WriterOwnership {
    /// OS process ID of the lock holder.
    pub pid: u32,
    /// RFC 3339 UTC timestamp when the lock was acquired.
    pub acquired_at: String,
}

/// Reason writer ownership could not be read.
#[derive(Debug, Eq, PartialEq)]
pub enum WriterOwnershipError {
    /// No lock file exists.
    NotFound,
    /// The lock file exists but is unreadable or malformed.
    Malformed(String),
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
    ownership: WriterOwnership,
}

impl WriterLock {
    /// Path to the lock file held by this guard.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for WriterLock {
    fn drop(&mut self) {
        let remaining = decrement_depth(&self.path);
        if remaining > 0 {
            // Not the last lock for this path; do not remove the file.
            return;
        }

        match read_ownership(&self.path) {
            Ok(owner) if owner == self.ownership => {
                if let Err(e) = fs::remove_file(&self.path) {
                    if e.kind() != std::io::ErrorKind::NotFound {
                        tracing::warn!(path = ?self.path, error = %e, "failed to remove writer lock file on drop");
                    }
                }
            }
            Ok(owner) => {
                tracing::debug!(
                    path = ?self.path,
                    current_pid = owner.pid,
                    current_acquired_at = %owner.acquired_at,
                    "writer lock file was replaced before drop; leaving current owner intact"
                );
            }
            Err(WriterOwnershipError::Malformed(_)) if self.path.exists() => {
                tracing::warn!(
                    path = ?self.path,
                    "writer lock file became unreadable before drop; leaving it in place"
                );
            }
            Err(_) => {}
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
                write_lock_file(&mut f, &lock_path, &json)?;
                increment_depth(&lock_path);
                return Ok(WriterLock {
                    path: lock_path,
                    ownership: ownership.clone(),
                });
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                // If the lock is already held by the current process, allow re-entrancy.
                if let Ok(owner) = read_ownership(&lock_path) {
                    if owner.pid == std::process::id() {
                        increment_depth(&lock_path);
                        return Ok(WriterLock {
                            path: lock_path,
                            ownership: owner,
                        });
                    }
                }

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
                // Retry briefly on parse failure: a concurrent writer may have
                // just won `create_new` and not yet flushed the ownership JSON.
                // Treating that empty window as "stale" would make a racing
                // acquirer remove the file mid-write and both callers would
                // then believe they hold the lock.
                match read_ownership_with_retry(&lock_path) {
                    Ok(owner) if owner.pid == std::process::id() => {
                        // Same process: allow re-entrancy even if the initial fast-path
                        // read raced with the concurrent write and returned Err.
                        increment_depth(&lock_path);
                        return Ok(WriterLock {
                            path: lock_path,
                            ownership: owner,
                        });
                    }
                    Ok(owner) if is_process_alive(owner.pid) => {
                        return Err(LockError::HeldByOther {
                            pid: owner.pid,
                            lock_path: lock_path.clone(),
                        });
                    }
                    _ => {
                        // Dead PID or unreadable (malformed/truncated): clear and retry.
                        if let Err(source) = fs::remove_file(&lock_path) {
                            if source.kind() != std::io::ErrorKind::NotFound {
                                return Err(LockError::Io {
                                    path: lock_path.clone(),
                                    source,
                                });
                            }
                        }
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
/// Returns `Err(WriterOwnershipError::NotFound)` when no lock file exists,
/// or `Err(WriterOwnershipError::Malformed)` when the file cannot be parsed.
/// Does not check whether the recorded PID is still alive.
pub fn current_ownership(synrepo_dir: &Path) -> Result<WriterOwnership, WriterOwnershipError> {
    read_ownership(&writer_lock_path(synrepo_dir))
}

/// Return the PID of a live foreign writer lock holder, if one exists.
///
/// Ignores locks owned by the current process and treats stale lock files as
/// absent so readers do not block on dead processes.
pub fn live_owner_pid(synrepo_dir: &Path) -> Option<u32> {
    let owner = current_ownership(synrepo_dir).ok()?;
    if owner.pid == std::process::id() {
        return None;
    }
    is_process_alive(owner.pid).then_some(owner.pid)
}

/// Canonical path of the writer lock file within `.synrepo/state/`.
pub fn writer_lock_path(synrepo_dir: &Path) -> PathBuf {
    synrepo_dir.join("state").join("writer.lock")
}

fn read_ownership(lock_path: &Path) -> Result<WriterOwnership, WriterOwnershipError> {
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
fn read_ownership_with_retry(lock_path: &Path) -> Result<WriterOwnership, WriterOwnershipError> {
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

fn write_lock_file(file: &mut fs::File, lock_path: &Path, json: &str) -> Result<(), LockError> {
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

/// Check whether a process is alive using `kill -0 <pid>` on Unix.
///
/// On non-Unix platforms, conservatively returns `true` (assumes alive) to
/// prevent spurious stale-lock takeover on untested operating systems.
//
// REVIEW NOTE: the `#[cfg(unix)]` / `#[cfg(not(unix))]` split is
// load-bearing. Collapsing the two branches would either (a) attempt to
// exec `kill` on Windows, which returns NotFound and then falls through to
// `unwrap_or(false)`, causing Windows writers to steal each other's locks,
// or (b) assume alive on Unix, which breaks stale-lock cleanup. Keep both
// branches; keep `unwrap_or(false)` strictly inside `cfg(unix)`.
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
mod tests;
