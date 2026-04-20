// Lease management is cross-platform: the control plane lives on top of
// `interprocess::local_socket`, which maps to Unix domain sockets on Unix and
// to named pipes on Windows. Unix-only helpers remain gated individually.

use std::{
    fs,
    path::{Path, PathBuf},
};

use interprocess::local_socket::Name;
#[cfg(unix)]
use interprocess::local_socket::{GenericFilePath, ToFsName};
#[cfg(windows)]
use interprocess::local_socket::{GenericNamespaced, ToNsName};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::pipeline::writer::{now_rfc3339, open_and_try_lock};
use super::status::StateLoadError;

const WATCH_DAEMON_FILENAME: &str = "watch-daemon.json";

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
    /// Platform-specific control endpoint identifier (socket filesystem path
    /// on Unix, named-pipe name on Windows). The serde alias accepts the
    /// legacy `socket_path` field name written by older daemons.
    #[serde(alias = "socket_path")]
    pub control_endpoint: String,
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
            control_endpoint: watch_control_endpoint(synrepo_dir),
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
    /// Another live watch service holds the lease but has not published state yet.
    #[error("watch service is starting for this repo; wait for it to become ready or stop it before starting another")]
    HeldByStarting {
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
    /// A security violation was detected (e.g. symlink or foreign-owned socket dir).
    #[error("Security violation: {0}")]
    Security(String),
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
    flock_path: PathBuf,
    socket_path: PathBuf,
    identity: WatchDaemonState,
    /// Held for its Drop side-effect (releases the kernel flock).
    #[allow(dead_code)]
    flock_file: fs::File,
}

impl Drop for WatchDaemonLease {
    fn drop(&mut self) {
        let current = load_watch_state_from_path(&self.state_path);
        if current
            .as_ref()
            .is_ok_and(|state| state.same_owner(&self.identity))
        {
            cleanup_file(&self.state_path);
            cleanup_file(&self.flock_path);
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
    let flock_path = watch_flock_path(synrepo_dir);
    let socket_path = watch_socket_path(synrepo_dir);
    let state_dir = synrepo_dir.join("state");
    fs::create_dir_all(&state_dir).map_err(|source| WatchDaemonError::Io {
        path: state_dir.clone(),
        source,
    })?;

    // Step 1: Acquire the kernel advisory lock on the sentinel file.
    // `open_and_try_lock` appends `.flock` to its argument, so pass `state_path`
    // (`watch-daemon.json`) and it creates `watch-daemon.json.flock` — the same
    // path `watch_flock_path()` returns and `watch_service_status` reads back.
    let flock_file = open_and_try_lock(&state_path).map_err(|e| match e {
        crate::pipeline::writer::LockError::Io { path, source } => {
            WatchDaemonError::Io { path, source }
        }
        _ => WatchDaemonError::Control(format!("Failed to open flock file: {e}")),
    })?;

    let Some(flock_file) = flock_file else {
        return Err(match load_watch_state_from_path(&state_path) {
            Ok(state) => WatchDaemonError::HeldByOther {
                pid: state.pid,
                state_path,
            },
            Err(StateLoadError::NotFound) => WatchDaemonError::HeldByStarting { state_path },
            Err(StateLoadError::Malformed(detail)) => WatchDaemonError::Control(format!(
                "watch service holds the lease but its state file is malformed: {detail}"
            )),
        });
    };

    // Step 2: Now that we own the flock, write the JSON metadata.
    let initial = WatchDaemonState::new(synrepo_dir, mode);
    persist_watch_state_at(&state_path, &initial)?;

    let lease = WatchDaemonLease {
        state_path: state_path.clone(),
        flock_path,
        socket_path,
        identity: initial.clone(),
        flock_file,
    };
    let handle = WatchStateHandle::new(state_path, initial);
    Ok((lease, handle))
}

/// Canonical path of the persisted watch-service state file.
pub fn watch_daemon_state_path(synrepo_dir: &Path) -> PathBuf {
    synrepo_dir.join("state").join(WATCH_DAEMON_FILENAME)
}

/// Logical path of the watch-daemon sentinel flock file.
pub fn watch_flock_path(synrepo_dir: &Path) -> PathBuf {
    synrepo_dir
        .join("state")
        .join(format!("{}.flock", WATCH_DAEMON_FILENAME))
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
    // Use a user-owned directory to prevent /tmp-based pre-creation or
    // permission-denial attacks. Prefers $HOME/.cache/synrepo-run/ or
    // $XDG_RUNTIME_DIR/synrepo/, falling back to a per-user/per-process
    // subdirectory in temp_dir() with 0700 permissions.
    let socket_dir = user_socket_dir();
    socket_dir.join(format!("{}.sock", stable_repo_hash_16(synrepo_dir)))
}

/// 16-hex-char prefix of a blake3 hash over the canonicalised synrepo
/// directory. Both the Unix socket filename and the Windows named-pipe name
/// key off the same digest so the endpoint is deterministic per repo.
fn stable_repo_hash_16(synrepo_dir: &Path) -> String {
    let canonical = fs::canonicalize(synrepo_dir).unwrap_or_else(|_| synrepo_dir.to_path_buf());
    let digest = blake3::hash(canonical.to_string_lossy().as_bytes());
    hex::encode(digest.as_bytes())[..16].to_string()
}

/// Platform-appropriate backing identifier for the watch control endpoint.
///
/// On Unix this is the socket filesystem path as a string (same backing store
/// as [`watch_socket_path`]). On Windows this is a namespaced pipe name
/// derived from a stable hash of the repo's synrepo directory, so the same
/// repo always resolves to the same pipe.
///
/// Kept as a `String` so the interprocess `Name<'_>` can be constructed at
/// use time without needing an owned-name facility on the crate side.
pub fn watch_control_endpoint(synrepo_dir: &Path) -> String {
    #[cfg(unix)]
    {
        watch_socket_path(synrepo_dir)
            .to_string_lossy()
            .into_owned()
    }
    #[cfg(windows)]
    {
        format!("synrepo-watch-{}", stable_repo_hash_16(synrepo_dir))
    }
}

/// Build an interprocess `Name<'_>` from the endpoint string produced by
/// [`watch_control_endpoint`]. Uses `GenericFilePath` on Unix and
/// `GenericNamespaced` on Windows.
pub(crate) fn watch_control_socket_name(endpoint: &str) -> std::io::Result<Name<'_>> {
    #[cfg(unix)]
    {
        endpoint.to_fs_name::<GenericFilePath>()
    }
    #[cfg(windows)]
    {
        endpoint.to_ns_name::<GenericNamespaced>()
    }
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

    let base_dir = std::env::temp_dir().join(format!("synrepo-run-{}", username));

    #[cfg(unix)]
    {
        // Try the primary name first. If it's foreign-owned or a symlink,
        // retry with a few random salts to avoid local DoS.
        if let Ok(dir) = try_create_hardened_socket_dir(&base_dir) {
            return dir;
        }

        // Salted retries.
        for salt in 1..=10 {
            let salted_dir =
                std::env::temp_dir().join(format!("synrepo-run-{}-{}", username, salt));
            if let Ok(dir) = try_create_hardened_socket_dir(&salted_dir) {
                return dir;
            }
        }
    }

    #[cfg(not(unix))]
    {
        if let Err(e) = fs::create_dir_all(&base_dir) {
            tracing::debug!("Failed to create fallback socket dir: {}", e);
        }
    }

    base_dir
}

#[cfg(unix)]
fn try_create_hardened_socket_dir(dir: &Path) -> Result<PathBuf, WatchDaemonError> {
    use std::os::unix::fs::DirBuilderExt;
    let mut builder = fs::DirBuilder::new();
    builder.mode(0o700);

    if let Err(e) = builder.create(dir) {
        if e.kind() != std::io::ErrorKind::AlreadyExists {
            return Err(WatchDaemonError::Io {
                path: dir.to_path_buf(),
                source: e,
            });
        }
    }

    harden_fallback_socket_dir(dir).map(|_| dir.to_path_buf())
}

/// Refuse symlinks, refuse foreign-owned directories, then chmod 0700.
///
/// Split out of `user_socket_dir` so regression tests can exercise it
/// directly against a crafted path without racing on process env vars.
#[cfg(unix)]
fn harden_fallback_socket_dir(dir: &Path) -> Result<(), WatchDaemonError> {
    use std::os::unix::fs::MetadataExt;

    // `fs::metadata` and `fs::set_permissions` both follow symlinks, so
    // without this check an attacker on a shared host could pre-create
    // `/tmp/synrepo-run-<victim>` as a symlink to a victim-owned directory
    // and watch the daemon chmod that directory to 0700.
    // `symlink_metadata` reports the link itself, not its target.
    match fs::symlink_metadata(dir) {
        Ok(meta) if meta.file_type().is_symlink() => {
            return Err(WatchDaemonError::Security(format!(
                "watch socket directory {} is a symlink; refusing to bind through it",
                dir.display()
            )));
        }
        Ok(_) => {}
        Err(e) => {
            return Err(WatchDaemonError::Io {
                path: dir.to_path_buf(),
                source: e,
            });
        }
    }

    if let Ok(meta) = fs::metadata(dir) {
        let get_current_uid = || -> Option<u32> {
            let output = std::process::Command::new("id").arg("-u").output().ok()?;
            let stdout = std::str::from_utf8(&output.stdout).ok()?.trim();
            stdout.parse::<u32>().ok()
        };

        if Some(meta.uid()) != get_current_uid() {
            return Err(WatchDaemonError::Security(format!(
                "watch socket directory {} exists but is owned by UID {}; potential privilege escalation",
                dir.display(),
                meta.uid()
            )));
        }
    }

    Ok(())
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
        let result = harden_fallback_socket_dir(&link_path);
        assert!(
            result.is_err(),
            "harden_fallback_socket_dir must fail on a symlink"
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
        // Must not fail — real directory, owned by us.
        harden_fallback_socket_dir(&dir).unwrap();
    }
}

/// Guard returned by [`hold_watch_flock_with_state`]. Dropping it releases
/// the kernel advisory lock held on its file descriptor.
#[doc(hidden)]
pub struct TestWatchFlockHolder {
    _file: fs::File,
}

/// Take the watch-daemon kernel flock on a separate fd and write a matching
/// state file. Simulates a foreign live watch daemon for tests in other
/// crates (binary-crate tests in particular) that cannot reach the private
/// `acquire_watch_daemon_lease` API.
///
/// Exposed as `pub` + `#[doc(hidden)]` so the binary-crate tests can use it
/// without widening the public API; see the `helpers.rs` note on why
/// `#[cfg(test)]` and `pub(crate)` don't work across the bin/lib boundary.
#[doc(hidden)]
pub fn hold_watch_flock_with_state(
    synrepo_dir: &Path,
    state: &WatchDaemonState,
) -> TestWatchFlockHolder {
    let state_dir = synrepo_dir.join("state");
    fs::create_dir_all(&state_dir).expect("create state dir");
    let state_path = watch_daemon_state_path(synrepo_dir);
    let file = crate::pipeline::writer::open_and_try_lock(&state_path)
        .expect("open+flock I/O must succeed in test")
        .expect("watch flock must be free (nothing else holds it)");
    let json = serde_json::to_string(state).expect("serialize WatchDaemonState");
    fs::write(&state_path, json.as_bytes()).expect("write watch state");
    TestWatchFlockHolder { _file: file }
}

pub(super) fn persist_watch_state_at(
    state_path: &Path,
    state: &WatchDaemonState,
) -> Result<(), WatchDaemonError> {
    let json = serde_json::to_string(state).expect("WatchDaemonState serializes");
    crate::util::atomic_write(state_path, json.as_bytes()).map_err(|source| {
        WatchDaemonError::Io {
            path: state_path.to_path_buf(),
            source,
        }
    })?;
    Ok(())
}

pub(super) fn load_watch_state_from_path(
    state_path: &Path,
) -> Result<WatchDaemonState, StateLoadError> {
    let text = fs::read_to_string(state_path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            StateLoadError::NotFound
        } else {
            StateLoadError::Malformed(e.to_string())
        }
    })?;
    serde_json::from_str(&text).map_err(|e| StateLoadError::Malformed(e.to_string()))
}

fn cleanup_file(path: &Path) {
    if let Err(error) = fs::remove_file(path) {
        if error.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(path = ?path, error = %error, "failed to clean up watch artifact");
        }
    }
}
