//! Internal helpers for the writer lock implementation.
//!
//! Contains re-entrancy tracking, kernel advisory locking, file I/O, process
//! liveness checks, and timestamp formatting. These are implementation details
//! not exposed in the public API.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    thread::ThreadId,
};

use fs2::FileExt;

use super::{LockError, WriterOwnership, WriterOwnershipError};

// ---- Re-entrancy tracking ----
//
// The open `File` that owns the kernel advisory lock is stored inside the
// ReentrancyState entry, so it is only dropped (and the kernel flock released)
// when the outermost `WriterLock` drops, regardless of the order in which
// re-entrant guards go out of scope. The ownership record is cached here too
// so re-entrant acquires don't re-read it from disk.

static LOCK_DEPTHS: OnceLock<Mutex<HashMap<PathBuf, ReentrancyState>>> = OnceLock::new();

struct ReentrancyState {
    depth: usize,
    owner_thread: ThreadId,
    /// Held for its Drop side-effect (releases the kernel flock). Never read.
    #[allow(dead_code)]
    file: fs::File,
    ownership: WriterOwnership,
}

fn lock_depths() -> &'static Mutex<HashMap<PathBuf, ReentrancyState>> {
    LOCK_DEPTHS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// If the current thread already holds the lock for `lock_path`, increment
/// the depth and return `Ok(Some(ownership))`. Return `Ok(None)` if no entry
/// exists. Return `Err(LockError::WrongThread)` if another thread holds it.
pub(super) fn try_reenter(lock_path: &Path) -> Result<Option<WriterOwnership>, LockError> {
    let mut map = lock_depths().lock().unwrap();
    let current = std::thread::current().id();
    match map.get_mut(lock_path) {
        Some(entry) if entry.owner_thread == current => {
            entry.depth += 1;
            Ok(Some(entry.ownership.clone()))
        }
        Some(_) => Err(LockError::WrongThread {
            lock_path: lock_path.to_path_buf(),
        }),
        None => Ok(None),
    }
}

/// Record the outermost lock acquisition for `lock_path`. Caller must already
/// hold the kernel advisory lock on `file`.
pub(super) fn insert_initial_lock(
    lock_path: &Path,
    file: fs::File,
    ownership: WriterOwnership,
) -> Result<(), LockError> {
    let mut map = lock_depths().lock().unwrap();
    let current = std::thread::current().id();
    if map.contains_key(lock_path) {
        // try_reenter is checked before we call this, so a hit here means the
        // depth map is out of sync with the kernel flock state. Refuse rather
        // than stomp the existing entry.
        return Err(LockError::WrongThread {
            lock_path: lock_path.to_path_buf(),
        });
    }
    map.insert(
        lock_path.to_path_buf(),
        ReentrancyState {
            depth: 1,
            owner_thread: current,
            file,
            ownership,
        },
    );
    Ok(())
}

/// Decrement depth for `lock_path` and return the value *after* decrement.
/// When depth reaches zero the entry is removed, which drops the stored
/// `File` and releases the kernel advisory lock.
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

// ---- Kernel advisory locking ----

// The kernel flock lives on a private sentinel file (`writer.lock.flock`)
// instead of the ownership-metadata file (`writer.lock`) itself. Windows
// `LockFileEx` is a byte-range lock that blocks reads from other handles
// (POSIX flock does not), so sharing one file for both roles makes the
// ownership record unreadable from another process while the lock is held.
// Splitting the two keeps the metadata freely readable on every platform.

/// Compute the sentinel path for a given metadata lock path.
pub(super) fn sentinel_path(lock_path: &Path) -> PathBuf {
    let mut p = lock_path.as_os_str().to_os_string();
    p.push(".flock");
    PathBuf::from(p)
}

/// Open (creating if needed) the sentinel file for `lock_path` and attempt
/// to acquire an exclusive non-blocking kernel advisory lock on it.
///
/// Returns `Ok(Some(file))` if we own the lock, `Ok(None)` if another
/// handle currently holds it, and `Err` for any other I/O failure.
///
/// On Unix the file is opened with `O_CLOEXEC` so child processes do not
/// inherit the lock; `LockFileEx` on Windows is per-handle by default.
pub(super) fn open_and_try_lock(lock_path: &Path) -> Result<Option<fs::File>, LockError> {
    let sentinel = sentinel_path(lock_path);
    let mut opts = fs::OpenOptions::new();
    opts.create(true).read(true).write(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.custom_flags(libc::O_CLOEXEC);
    }

    let file = opts.open(&sentinel).map_err(|source| LockError::Io {
        path: sentinel.clone(),
        source,
    })?;

    match file.try_lock_exclusive() {
        Ok(()) => Ok(Some(file)),
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
        Err(source) => Err(LockError::Io {
            path: sentinel,
            source,
        }),
    }
}

/// Same as [`open_and_try_lock`] but retries briefly on `WouldBlock`.
///
/// On macOS under heavy parallel load, `close(fd)` can return before the
/// kernel has propagated the flock release to a concurrently-opening fd. A
/// short backoff distinguishes that transient state from genuine contention
/// with a live holder; the kernel path is otherwise unchanged.
pub(super) fn open_and_try_lock_with_retry(
    lock_path: &Path,
) -> Result<Option<fs::File>, LockError> {
    if let Some(file) = open_and_try_lock(lock_path)? {
        return Ok(Some(file));
    }
    const RETRIES: u32 = 20;
    const BACKOFF_MS: u64 = 5;
    for _ in 0..RETRIES {
        std::thread::sleep(std::time::Duration::from_millis(BACKOFF_MS));
        if let Some(file) = open_and_try_lock(lock_path)? {
            return Ok(Some(file));
        }
    }
    Ok(None)
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
/// window where a concurrent flock winner has acquired the kernel lock but
/// has not yet written its ownership metadata. The total wait is sized to
/// cover thread-scheduling jitter on slow Windows CI runners.
pub(super) fn read_ownership_with_retry(
    lock_path: &Path,
) -> Result<WriterOwnership, WriterOwnershipError> {
    const RETRIES: u32 = 50;
    const BACKOFF_MS: u64 = 5;
    for _ in 0..RETRIES {
        if let Ok(owner) = read_ownership(lock_path) {
            return Ok(owner);
        }
        std::thread::sleep(std::time::Duration::from_millis(BACKOFF_MS));
    }
    read_ownership(lock_path)
}

/// Write ownership JSON to the metadata lock file. The metadata file is
/// separate from the kernel-flocked sentinel, so this is a plain truncating
/// write with no file-handle coordination.
pub(super) fn write_lock_metadata(lock_path: &Path, json: &str) -> Result<(), LockError> {
    if let Err(source) = fs::write(lock_path, json.as_bytes()) {
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

// ---- Test-only helpers ----
//
// Exposed as `pub` + `#[doc(hidden)]` (not `#[cfg(test)]`) because binary-crate
// tests compile against the library *without* `cfg(test)` and so cannot see
// test-gated items. Doc-hidden keeps them out of the public rustdoc surface.

/// Spawn a short-lived child and reap it, returning its (now-dead) PID.
#[cfg(unix)]
#[doc(hidden)]
pub fn spawn_and_reap_pid() -> u32 {
    let mut child = std::process::Command::new("true")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn true");
    let pid = child.id();
    child.wait().expect("wait for child");
    pid
}

/// Spawn a long-sleeping child and return its (Child, live pid). The caller
/// must keep the Child alive until the PID is no longer needed, and should
/// kill/wait it to avoid leaked zombies (callers that rely on unwinding
/// cleanup are fine — Child's Drop does not kill).
#[cfg(unix)]
#[doc(hidden)]
pub fn live_foreign_pid() -> (std::process::Child, u32) {
    let child = std::process::Command::new("sleep")
        .arg("30")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn sleep");
    let pid = child.id();
    (child, pid)
}

/// Guard returned by [`hold_writer_flock_with_ownership`]. Dropping it
/// releases the kernel advisory lock held on its file descriptor.
#[cfg(unix)]
#[doc(hidden)]
pub struct TestFlockHolder {
    _file: fs::File,
}

/// Open the sentinel file on a separate fd, take the kernel advisory lock,
/// and stamp ownership metadata. Used by tests to simulate a foreign writer:
/// same-process, different open file description, which blocks
/// `try_lock_exclusive` on any other fd exactly like a separate process would.
#[cfg(unix)]
#[doc(hidden)]
pub fn hold_writer_flock_with_ownership(
    lock_path: &Path,
    ownership: &WriterOwnership,
) -> TestFlockHolder {
    if let Some(parent) = lock_path.parent() {
        fs::create_dir_all(parent).expect("create state dir");
    }
    let file = open_and_try_lock(lock_path)
        .expect("open+flock I/O must succeed in test")
        .expect("flock must be free (nothing else holds it)");
    let json = serde_json::to_string(ownership).expect("serialize ownership");
    write_lock_metadata(lock_path, &json).expect("write ownership");
    TestFlockHolder { _file: file }
}
