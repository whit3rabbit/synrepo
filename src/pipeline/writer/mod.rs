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
//! 2. Mutual exclusion is enforced by a kernel advisory lock (`flock` on
//!    Unix, `LockFileEx` on Windows) held on `.synrepo/state/writer.lock`.
//!    If another live process holds the kernel lock, `acquire_writer_lock`
//!    returns `Err(LockError::HeldByOther)`.
//! 3. When the previous holder terminates, the kernel releases its flock
//!    automatically, so there is no file-existence TOCTOU race and no
//!    stale-file cleanup retry loop.
//! 4. The lock is released when `WriterLock` is dropped; the Drop impl
//!    drops the file handle (releasing the kernel flock) and removes the
//!    on-disk file.
//!
//! ## Daemon handoff
//!
//! In standalone CLI mode, the CLI process acquires and releases the lock on
//! each write operation. A future daemon-assisted mode would hold the lock for
//! its full lifetime, and CLI processes would read-only or delegate writes to
//! the daemon rather than competing for the lock.

mod helpers;

#[cfg(test)]
mod tests;

use std::{
    fs,
    path::{Path, PathBuf},
};

pub(super) use helpers::now_rfc3339;
use helpers::{
    decrement_depth, insert_initial_lock, open_and_try_lock_with_retry, read_ownership,
    read_ownership_with_retry, try_reenter, write_lock_metadata,
};
#[cfg(unix)]
pub use helpers::{
    hold_writer_flock_with_ownership, live_foreign_pid, spawn_and_reap_pid, TestFlockHolder,
};
pub(crate) use helpers::{is_process_alive, open_and_try_lock};
use serde::{Deserialize, Serialize};

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
    /// A different thread in the same process already holds the lock.
    #[error("writer lock held by another thread in this process; cross-thread re-entry is not permitted")]
    WrongThread {
        /// Path to the lock file.
        lock_path: PathBuf,
    },
    /// A watch service is authoritative for this repo and delegation is not
    /// supported for the requested operation.
    #[error("watch service is active for this repo (pid {watch_pid}); stop the watch daemon before running mutating commands")]
    WatchOwned {
        /// PID of the active watch service.
        watch_pid: u32,
    },
    /// The lock file exists but is unreadable or malformed.
    #[error("writer lock at {lock_path} is malformed: {detail}")]
    Malformed {
        /// Path to the lock file.
        lock_path: PathBuf,
        /// Description of the malformation.
        detail: String,
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
            return;
        }

        // `decrement_depth` already released the kernel flock. Remove the
        // on-disk file so idle repos report NotFound, but only if it still
        // carries our ownership record: tests and tooling sometimes replace
        // the metadata, and that replacement must survive our drop.
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
/// Mutual exclusion is provided by a kernel advisory lock (`flock` on Unix,
/// `LockFileEx` on Windows). The kernel releases a dead holder's lock on
/// process exit, so this function never needs to race on file-existence
/// cleanup.
pub fn acquire_writer_lock(synrepo_dir: &Path) -> Result<WriterLock, LockError> {
    let lock_path = writer_lock_path(synrepo_dir);
    let state_dir = synrepo_dir.join("state");

    fs::create_dir_all(&state_dir).map_err(|source| LockError::Io {
        path: state_dir.clone(),
        source,
    })?;

    // Fast path: same thread already holds the lock — just bump the depth.
    // Ownership is cached in the depth map, so no disk read here.
    if let Some(ownership) = try_reenter(&lock_path)? {
        return Ok(WriterLock {
            path: lock_path,
            ownership,
        });
    }

    let Some(file) = open_and_try_lock_with_retry(&lock_path)? else {
        // Another fd holds the kernel flock. Read ownership JSON for a useful
        // error message; retry briefly to ride out the window between a
        // winner's flock acquire and its ownership write.
        let pid = read_ownership_with_retry(&lock_path)
            .map(|o| o.pid)
            .unwrap_or(0);
        // Our own pid here means a different thread in this process holds the
        // flock on a non-registered fd (test shim or invariant violation);
        // same-thread re-entry would have been served by try_reenter above.
        if pid == std::process::id() {
            return Err(LockError::WrongThread {
                lock_path: lock_path.clone(),
            });
        }
        return Err(LockError::HeldByOther {
            pid,
            lock_path: lock_path.clone(),
        });
    };

    // Stamp ownership metadata. `fs::write` truncates any stale content from a
    // prior (dead) holder. The metadata file is distinct from the flocked
    // sentinel that `open_and_try_lock` returned, so this is a plain write.
    let ownership = WriterOwnership {
        pid: std::process::id(),
        acquired_at: now_rfc3339(),
    };
    let json = serde_json::to_string(&ownership).expect("WriterOwnership serializes without error");
    write_lock_metadata(&lock_path, &json)?;

    insert_initial_lock(&lock_path, file, ownership.clone())?;

    Ok(WriterLock {
        path: lock_path,
        ownership,
    })
}

/// Unified write-admission entry point for mutating CLI operations.
///
/// Checks watch ownership, cleans up stale artifacts, and acquires the writer
/// lock. All mutating CLI paths should use this instead of separately calling
/// `ensure_watch_not_running` + `acquire_writer_lock`.
pub fn acquire_write_admission(
    synrepo_dir: &Path,
    operation: &str,
) -> Result<WriterLock, LockError> {
    use crate::pipeline::watch::{
        cleanup_stale_watch_artifacts, watch_service_status, WatchServiceStatus,
    };

    tracing::debug!(operation, "acquiring write admission");

    match watch_service_status(synrepo_dir) {
        WatchServiceStatus::Inactive => {}
        WatchServiceStatus::Stale(_) | WatchServiceStatus::Corrupt(_) => {
            if let Err(e) = cleanup_stale_watch_artifacts(synrepo_dir) {
                tracing::warn!("failed to clean stale watch artifacts: {e}");
            }
        }
        WatchServiceStatus::Running(state) => {
            return Err(LockError::WatchOwned {
                watch_pid: state.pid,
            });
        }
    }

    acquire_writer_lock(synrepo_dir)
}

/// Map a `LockError` to a user-facing `anyhow::Error` prefixed with an
/// operation label. Covers every `LockError` variant exhaustively; callers
/// must not add wildcard arms around this.
pub fn map_lock_error(operation: &'static str, err: LockError) -> anyhow::Error {
    match err {
        LockError::HeldByOther { pid, .. } => anyhow::anyhow!(
            "{operation}: writer lock held by pid {pid}; wait for it to finish or stop the watch daemon"
        ),
        LockError::WatchOwned { watch_pid } => anyhow::anyhow!(
            "{operation}: watch service is active (pid {watch_pid}); run `synrepo watch stop` first"
        ),
        LockError::Io { path, source } => anyhow::anyhow!(
            "{operation}: could not acquire writer lock at {}: {source}",
            path.display()
        ),
        LockError::WrongThread { .. } => anyhow::anyhow!(
            "{operation}: writer lock already held by another thread in this process"
        ),
        LockError::Malformed { lock_path, detail } => anyhow::anyhow!(
            "{operation}: writer lock at {} is malformed ({detail}); remove the file and retry",
            lock_path.display()
        ),
    }
}

/// Read current writer ownership from the lock file, if present and readable.
pub fn current_ownership(synrepo_dir: &Path) -> Result<WriterOwnership, WriterOwnershipError> {
    read_ownership(&writer_lock_path(synrepo_dir))
}

/// Return the PID of a live foreign writer lock holder, if one exists.
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
