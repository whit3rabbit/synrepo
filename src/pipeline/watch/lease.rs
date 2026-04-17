// Lease management is only active on unix (watch daemon requires Unix sockets).
#![cfg_attr(not(unix), allow(dead_code, unused_imports))]

use std::{
    fs,
    io::Write as _,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::pipeline::writer::{is_process_alive, now_rfc3339};

const WATCH_DAEMON_FILENAME: &str = "watch-daemon.json";
static NEXT_WATCH_STATE_TMP_ID: AtomicU64 = AtomicU64::new(0);

/// Foreground or daemon execution mode for the watch service.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchServiceMode {
    /// Running in the current `synrepo watch` process.
    Foreground,
    /// Running in a detached helper process started by `synrepo watch --daemon`.
    Daemon,
}

impl std::fmt::Display for WatchServiceMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Foreground => f.write_str("foreground"),
            Self::Daemon => f.write_str("daemon"),
        }
    }
}

/// Persisted watch-service ownership and telemetry record.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WatchDaemonState {
    /// OS process ID of the service owner.
    pub pid: u32,
    /// RFC 3339 UTC timestamp when the service acquired the lease.
    pub started_at: String,
    /// Whether the service is foreground or detached.
    pub mode: WatchServiceMode,
    /// Canonical path of the control socket for this repo.
    pub socket_path: String,
    /// Most recent filesystem event burst seen outside `.synrepo/`.
    pub last_event_at: Option<String>,
    /// RFC 3339 UTC timestamp of the last completed reconcile attempt.
    pub last_reconcile_at: Option<String>,
    /// Stable outcome string from the last reconcile attempt.
    pub last_reconcile_outcome: Option<String>,
    /// Last reconcile error message, if any.
    pub last_error: Option<String>,
    /// Number of triggering events in the last reconcile burst.
    pub last_triggering_events: Option<usize>,
}

impl WatchDaemonState {
    fn new(synrepo_dir: &Path, mode: WatchServiceMode) -> Self {
        Self {
            pid: std::process::id(),
            started_at: now_rfc3339(),
            mode,
            socket_path: watch_socket_path(synrepo_dir).display().to_string(),
            last_event_at: None,
            last_reconcile_at: None,
            last_reconcile_outcome: None,
            last_error: None,
            last_triggering_events: None,
        }
    }

    fn same_owner(&self, other: &Self) -> bool {
        self.pid == other.pid && self.started_at == other.started_at
    }
}

/// Current watch-service state for a repo.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WatchServiceStatus {
    /// No watch service lease exists.
    Inactive,
    /// A live watch service owns the repo.
    Running(WatchDaemonState),
    /// A stale lease or socket remains from a dead service.
    Stale(Option<WatchDaemonState>),
    /// The lease file exists but is malformed.
    Corrupt(String),
}

/// Reason watch or reconcile state could not be loaded.
#[derive(Debug, Eq, PartialEq)]
pub enum StateLoadError {
    /// No state file exists.
    NotFound,
    /// The state file exists but is unreadable or malformed.
    Malformed(String),
}

/// Errors raised by the watch daemon lease or control plane.
#[derive(Debug, thiserror::Error)]
pub enum WatchDaemonError {
    /// Another live watch service owns the repo.
    #[error("watch service already running under pid {pid}; stop it before starting another")]
    HeldByOther {
        /// PID of the current owner.
        pid: u32,
        /// Path to the lease file.
        state_path: PathBuf,
    },
    /// I/O error touching the lease or socket.
    #[error("I/O error at {path}: {source}")]
    Io {
        /// Path where the error occurred.
        path: PathBuf,
        #[source]
        /// Underlying I/O error.
        source: std::io::Error,
    },
    /// Control request failed.
    #[error("{0}")]
    Control(String),
}

/// Shared mutable service telemetry.
#[derive(Clone, Debug)]
pub struct WatchStateHandle {
    state_path: PathBuf,
    state: std::sync::Arc<Mutex<WatchDaemonState>>,
}

impl WatchStateHandle {
    pub(crate) fn new(state_path: PathBuf, initial: WatchDaemonState) -> Self {
        Self {
            state_path,
            state: std::sync::Arc::new(Mutex::new(initial)),
        }
    }

    /// Return the latest in-memory snapshot.
    pub fn snapshot(&self) -> WatchDaemonState {
        self.state.lock().clone()
    }

    /// Record that a filesystem burst outside `.synrepo/` was observed.
    pub fn note_event(&self) {
        let snapshot = {
            let mut state = self.state.lock();
            state.last_event_at = Some(now_rfc3339());
            state.clone()
        };
        let _ = persist_watch_state_at(&self.state_path, &snapshot);
    }

    /// Record the latest reconcile outcome.
    pub fn note_reconcile(
        &self,
        outcome: &super::reconcile::ReconcileOutcome,
        triggering_events: usize,
    ) {
        let snapshot = {
            let mut state = self.state.lock();
            state.last_reconcile_at = Some(now_rfc3339());
            state.last_reconcile_outcome = Some(outcome.as_str().to_string());
            state.last_triggering_events = Some(triggering_events);
            state.last_error = match outcome {
                super::reconcile::ReconcileOutcome::Failed(message) => Some(message.clone()),
                _ => None,
            };
            state.clone()
        };
        let _ = persist_watch_state_at(&self.state_path, &snapshot);
    }
}

/// RAII guard holding the per-repo watch-service lease.
#[derive(Debug)]
pub struct WatchDaemonLease {
    state_path: PathBuf,
    socket_path: PathBuf,
    identity: WatchDaemonState,
}

impl Drop for WatchDaemonLease {
    fn drop(&mut self) {
        let current = load_watch_state_from_path(&self.state_path);
        if current
            .as_ref()
            .is_ok_and(|state| state.same_owner(&self.identity))
        {
            cleanup_file(&self.state_path);
            cleanup_file(&self.socket_path);
        }
    }
}

/// Acquire the per-repo watch-service lease and write the initial state file.
pub(crate) fn acquire_watch_daemon_lease(
    synrepo_dir: &Path,
    mode: WatchServiceMode,
) -> Result<(WatchDaemonLease, WatchStateHandle), WatchDaemonError> {
    let state_path = watch_daemon_state_path(synrepo_dir);
    let socket_path = watch_socket_path(synrepo_dir);
    let state_dir = synrepo_dir.join("state");
    fs::create_dir_all(&state_dir).map_err(|source| WatchDaemonError::Io {
        path: state_dir.clone(),
        source,
    })?;

    let initial = WatchDaemonState::new(synrepo_dir, mode);
    let json = serde_json::to_string(&initial).expect("WatchDaemonState serializes");
    let mut cleared_stale = false;

    loop {
        match fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&state_path)
        {
            Ok(mut file) => {
                if let Err(source) = file.write_all(json.as_bytes()) {
                    cleanup_file(&state_path);
                    return Err(WatchDaemonError::Io {
                        path: state_path.clone(),
                        source,
                    });
                }
                if let Err(source) = file.sync_all() {
                    cleanup_file(&state_path);
                    return Err(WatchDaemonError::Io {
                        path: state_path.clone(),
                        source,
                    });
                }
                let lease = WatchDaemonLease {
                    state_path: state_path.clone(),
                    socket_path,
                    identity: initial.clone(),
                };
                let handle = WatchStateHandle::new(state_path, initial);
                return Ok((lease, handle));
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if cleared_stale {
                    return Err(
                        match load_watch_state_from_path(&watch_daemon_state_path(synrepo_dir)) {
                            Ok(state) => WatchDaemonError::HeldByOther {
                                pid: state.pid,
                                state_path: watch_daemon_state_path(synrepo_dir),
                            },
                            Err(_) => WatchDaemonError::Io {
                                path: watch_daemon_state_path(synrepo_dir),
                                source: error,
                            },
                        },
                    );
                }

                match load_watch_state_with_retry(&watch_daemon_state_path(synrepo_dir)) {
                    Ok(state) if is_process_alive(state.pid) => {
                        return Err(WatchDaemonError::HeldByOther {
                            pid: state.pid,
                            state_path: watch_daemon_state_path(synrepo_dir),
                        });
                    }
                    _ => {
                        cleanup_file(&watch_daemon_state_path(synrepo_dir));
                        cleanup_file(&watch_socket_path(synrepo_dir));
                        cleared_stale = true;
                    }
                }
            }
            Err(source) => {
                return Err(WatchDaemonError::Io {
                    path: watch_daemon_state_path(synrepo_dir),
                    source,
                });
            }
        }
    }
}

/// Load the persisted watch-service state, if present and readable.
pub fn load_watch_state(synrepo_dir: &Path) -> Result<WatchDaemonState, StateLoadError> {
    load_watch_state_from_path(&watch_daemon_state_path(synrepo_dir))
}

/// Inspect whether the repo currently has a live, stale, or missing watch service.
pub fn watch_service_status(synrepo_dir: &Path) -> WatchServiceStatus {
    let state_path = watch_daemon_state_path(synrepo_dir);
    if !state_path.exists() {
        return WatchServiceStatus::Inactive;
    }

    match load_watch_state_from_path(&state_path) {
        Ok(state) if is_process_alive(state.pid) => WatchServiceStatus::Running(state),
        Ok(state) => WatchServiceStatus::Stale(Some(state)),
        Err(StateLoadError::NotFound) => WatchServiceStatus::Inactive,
        Err(StateLoadError::Malformed(e)) => WatchServiceStatus::Corrupt(e),
    }
}

/// Remove stale watch-service artifacts left behind by a dead process.
///
/// Also sweeps an orphan control socket when no state file exists: a daemon
/// that died after `bind()` but before `watch-daemon.json` was written leaves
/// the socket behind, and without this pass the next `synrepo watch` fails
/// with "Address already in use".
pub fn cleanup_stale_watch_artifacts(synrepo_dir: &Path) -> Result<bool, WatchDaemonError> {
    match watch_service_status(synrepo_dir) {
        WatchServiceStatus::Running(_) => Ok(false),
        WatchServiceStatus::Inactive => remove_ignore_missing(watch_socket_path(synrepo_dir)),
        WatchServiceStatus::Stale(_) | WatchServiceStatus::Corrupt(_) => {
            remove_ignore_missing(watch_daemon_state_path(synrepo_dir))?;
            remove_ignore_missing(watch_socket_path(synrepo_dir))?;
            Ok(true)
        }
    }
}

fn remove_ignore_missing(path: PathBuf) -> Result<bool, WatchDaemonError> {
    match fs::remove_file(&path) {
        Ok(()) => Ok(true),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(source) => Err(WatchDaemonError::Io { path, source }),
    }
}

/// Canonical path of the persisted watch-service state file.
pub fn watch_daemon_state_path(synrepo_dir: &Path) -> PathBuf {
    synrepo_dir.join("state").join(WATCH_DAEMON_FILENAME)
}

/// Canonical path of the per-repo watch control socket.
///
/// Resolves to `<temp_dir>/synrepo-<hash>.sock`, not inside `.synrepo/state/`.
/// The Unix domain socket path must fit inside `sockaddr_un.sun_path`, which
/// is capped at 104 bytes on macOS and 108 on Linux (NUL inclusive). A deep
/// repo path such as
/// `/Users/<name>/Documents/<client>/<monorepo>/<service>/.synrepo/state/watch.sock`
/// blows past that limit and `UnixListener::bind` returns ENAMETOOLONG with
/// no useful diagnostic, so we hash the canonicalised synrepo directory and
/// place the socket in `std::env::temp_dir()`, which is short on every
/// supported platform.
///
/// The repo_dir → socket mapping is deterministic: the same repo always
/// gets the same socket path, so clients reading `watch-daemon.json` and
/// reconnecting see a stable endpoint. A canonicalisation failure falls
/// back to the literal input, which is still consistent for the lifetime
/// of one synrepo install.
pub fn watch_socket_path(synrepo_dir: &Path) -> PathBuf {
    let canonical = fs::canonicalize(synrepo_dir).unwrap_or_else(|_| synrepo_dir.to_path_buf());
    let digest = blake3::hash(canonical.to_string_lossy().as_bytes());
    let hex = hex::encode(digest.as_bytes());

    // Use a user-owned directory to prevent /tmp-based pre-creation or
    // permission-denial attacks. Prefers $HOME/.cache/synrepo-run/ or
    // $XDG_RUNTIME_DIR/synrepo/, falling back to a per-user/per-process
    // subdirectory in temp_dir() with 0700 permissions.
    let socket_dir = user_socket_dir();
    socket_dir.join(format!("{}.sock", &hex[..16]))
}

fn user_socket_dir() -> PathBuf {
    // 1. $HOME/.cache/synrepo-run/ (Standard persistent user cache)
    if let Ok(home) = std::env::var("HOME") {
        let dir = PathBuf::from(home).join(".cache").join("synrepo-run");
        if fs::create_dir_all(&dir).is_ok() {
            return dir;
        }
    }

    // 2. $XDG_RUNTIME_DIR/synrepo/ (Standard Unix runtime dir)
    if let Ok(runtime) = std::env::var("XDG_RUNTIME_DIR") {
        let dir = PathBuf::from(runtime).join("synrepo");
        if fs::create_dir_all(&dir).is_ok() {
            return dir;
        }
    }

    // 3. Fallback: temp_dir() with a user-bound name and 0700 permissions.
    let username = std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| "unknown".to_string());
    let dir = std::env::temp_dir().join(format!("synrepo-run-{}", username));

    if let Err(e) = fs::create_dir_all(&dir) {
        tracing::debug!("Failed to create fallback socket dir: {}", e);
    }

    #[cfg(unix)]
    harden_fallback_socket_dir(&dir);

    dir
}

/// Refuse symlinks, refuse foreign-owned directories, then chmod 0700.
///
/// Split out of `user_socket_dir` so regression tests can exercise it
/// directly against a crafted path without racing on process env vars.
///
/// Panics on symlink or foreign ownership — the `/tmp` fallback is a
/// security boundary and silently continuing would let an attacker redirect
/// or chmod victim-owned directories.
#[cfg(unix)]
fn harden_fallback_socket_dir(dir: &Path) {
    use std::os::unix::fs::MetadataExt;
    use std::os::unix::fs::PermissionsExt;

    // `fs::metadata` and `fs::set_permissions` both follow symlinks, so
    // without this check an attacker on a shared host could pre-create
    // `/tmp/synrepo-run-<victim>` as a symlink to a victim-owned directory
    // and watch the daemon chmod that directory to 0700.
    // `symlink_metadata` reports the link itself, not its target.
    match fs::symlink_metadata(dir) {
        Ok(meta) if meta.file_type().is_symlink() => {
            panic!(
                "Security violation: watch socket directory {} is a symlink; \
                 refusing to chmod or bind through it.",
                dir.display()
            );
        }
        Ok(_) => {}
        Err(e) => {
            tracing::debug!("Failed to stat fallback socket dir: {}", e);
            return;
        }
    }

    if let Ok(meta) = fs::metadata(dir) {
        let get_current_uid = || -> Option<u32> {
            let output = std::process::Command::new("id").arg("-u").output().ok()?;
            let stdout = std::str::from_utf8(&output.stdout).ok()?.trim();
            stdout.parse::<u32>().ok()
        };

        if Some(meta.uid()) != get_current_uid() {
            panic!(
                "Security violation: watch socket directory {} exists but is owned by UID {}. \
                 This indicates a potential privilege escalation attempt.",
                dir.display(),
                meta.uid()
            );
        }
    }

    if let Err(e) = fs::set_permissions(dir, fs::Permissions::from_mode(0o700)) {
        tracing::debug!("Failed to set permissions on fallback socket dir: {}", e);
    }
}

#[cfg(all(test, unix))]
mod lease_security_tests {
    use super::harden_fallback_socket_dir;
    use std::fs;
    use std::os::unix::fs::symlink;
    use tempfile::tempdir;

    #[test]
    fn harden_refuses_symlinks() {
        let outer = tempdir().unwrap();
        // Real directory the attacker wants to attack via symlink.
        let target = outer.path().join("victim-dir");
        fs::create_dir_all(&target).unwrap();
        // Attacker-controlled symlink in a writable location.
        let link = outer.path().join("synrepo-run-victim");
        symlink(&target, &link).unwrap();

        let link_path = link.clone();
        let result = std::panic::catch_unwind(|| harden_fallback_socket_dir(&link_path));
        assert!(
            result.is_err(),
            "harden_fallback_socket_dir must panic on a symlink"
        );

        // Target's mode must NOT have been changed to 0o700. Since we created
        // `target` with the process default (typically 0o755), a successful
        // attack would have chmod'd it. Just assert the target still exists.
        assert!(target.exists());
    }

    #[test]
    fn harden_accepts_real_directory_owned_by_current_user() {
        let outer = tempdir().unwrap();
        let dir = outer.path().join("synrepo-run-self");
        fs::create_dir_all(&dir).unwrap();
        // Must not panic — real directory, owned by us.
        harden_fallback_socket_dir(&dir);
    }
}

pub(super) fn persist_watch_state_at(
    state_path: &Path,
    state: &WatchDaemonState,
) -> Result<(), WatchDaemonError> {
    let json = serde_json::to_string(state).expect("WatchDaemonState serializes");
    let state_dir = state_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let tmp_path = watch_state_tmp_path(&state_dir);
    fs::write(&tmp_path, json.as_bytes())
        .and_then(|_| fs::rename(&tmp_path, state_path))
        .map_err(|source| WatchDaemonError::Io {
            path: state_path.to_path_buf(),
            source,
        })?;
    Ok(())
}

fn load_watch_state_from_path(state_path: &Path) -> Result<WatchDaemonState, StateLoadError> {
    let text = fs::read_to_string(state_path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            StateLoadError::NotFound
        } else {
            StateLoadError::Malformed(e.to_string())
        }
    })?;
    serde_json::from_str(&text).map_err(|e| StateLoadError::Malformed(e.to_string()))
}

fn load_watch_state_with_retry(state_path: &Path) -> Result<WatchDaemonState, StateLoadError> {
    const RETRIES: u32 = 5;
    const BACKOFF_MS: u64 = 1;
    for _ in 0..RETRIES {
        if let Ok(state) = load_watch_state_from_path(state_path) {
            return Ok(state);
        }
        std::thread::sleep(Duration::from_millis(BACKOFF_MS));
    }
    load_watch_state_from_path(state_path)
}

fn watch_state_tmp_path(state_dir: &Path) -> PathBuf {
    let id = NEXT_WATCH_STATE_TMP_ID.fetch_add(1, Ordering::Relaxed);
    state_dir.join(format!(
        "{WATCH_DAEMON_FILENAME}.tmp.{}.{}",
        std::process::id(),
        id
    ))
}

fn cleanup_file(path: &Path) {
    if let Err(error) = fs::remove_file(path) {
        if error.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(path = ?path, error = %error, "failed to clean up watch artifact");
        }
    }
}
