// Lease management is cross-platform: the control plane lives on top of
// `interprocess::local_socket`, which maps to Unix domain sockets on Unix and
// to named pipes on Windows. Unix-only helpers remain gated individually.

mod paths;
mod types;

use std::{fs, path::Path};

use super::status::StateLoadError;
use crate::pipeline::writer::open_and_try_lock;

// Re-export all public items from submodules so external crate paths remain
// unchanged.
pub use paths::{
    watch_control_endpoint, watch_control_socket_name, watch_daemon_state_path, watch_flock_path,
    watch_socket_path,
};
pub use types::{
    TestWatchFlockHolder, WatchDaemonError, WatchDaemonLease, WatchDaemonState, WatchServiceMode,
    WatchStateHandle,
};

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
    // (`watch-daemon.json`) and it creates `watch-daemon.json.flock` -- the same
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
    let json = serde_json::to_string(state).map_err(|e| WatchDaemonError::Io {
        path: state_path.to_path_buf(),
        source: std::io::Error::other(format!("serialize WatchDaemonState failed: {e}")),
    })?;
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

pub(super) fn cleanup_file(path: &Path) {
    if let Err(error) = fs::remove_file(path) {
        if error.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(path = ?path, error = %error, "failed to clean up watch artifact");
        }
    }
}

#[cfg(all(test, unix))]
mod lease_security_tests {
    use super::paths::harden_fallback_socket_dir;
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
        // Must not fail -- real directory, owned by us.
        harden_fallback_socket_dir(&dir).unwrap();
    }

    #[test]
    fn harden_enforces_mode_0700_on_existing_directory() {
        use std::os::unix::fs::PermissionsExt;
        let outer = tempdir().unwrap();
        let dir = outer.path().join("synrepo-run-self");
        fs::create_dir_all(&dir).unwrap();
        // Simulate a pre-existing directory left in a wider mode (the
        // AlreadyExists branch in try_create_hardened_socket_dir would skip
        // DirBuilder::mode and rely on harden_fallback_socket_dir to retighten).
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
        harden_fallback_socket_dir(&dir).unwrap();
        let mode = fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o700,
            "harden_fallback_socket_dir must chmod the dir to 0o700, got {mode:o}"
        );
    }
}
