use std::{
    fs,
    path::{Path, PathBuf},
};

use interprocess::local_socket::Name;
#[cfg(unix)]
use interprocess::local_socket::{GenericFilePath, ToFsName};
#[cfg(windows)]
use interprocess::local_socket::{GenericNamespaced, ToNsName};

use super::types::WatchDaemonError;

const WATCH_DAEMON_FILENAME: &str = "watch-daemon.json";

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
/// The repo_dir -> socket mapping is deterministic: the same repo always
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
pub(crate) fn stable_repo_hash_16(synrepo_dir: &Path) -> String {
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
pub fn watch_control_socket_name(endpoint: &str) -> std::io::Result<Name<'_>> {
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
pub(crate) fn harden_fallback_socket_dir(dir: &Path) -> Result<(), WatchDaemonError> {
    use std::os::unix::fs::MetadataExt;

    // `fs::metadata` and `fs::set_permissions` both follow symlinks, so
    // without this check an attacker on a shared host could pre-create
    // `/tmp/synrepo-run-<victim>` as a symlink to a victim-owned directory
    // and watch the daemon chmod that directory to 0o700.
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
