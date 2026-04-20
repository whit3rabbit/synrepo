use std::{fs, path::PathBuf};

use fs2::FileExt;

use super::lease::{
    load_watch_state_from_path, watch_daemon_state_path, watch_flock_path, watch_socket_path,
    WatchDaemonError, WatchDaemonState,
};

/// Current watch-service state for a repo.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WatchServiceStatus {
    /// No watch service lease exists.
    Inactive,
    /// A live watch service has the lease but has not published state yet.
    Starting,
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

/// Load the persisted watch-service state, if present and readable.
pub fn load_watch_state(synrepo_dir: &std::path::Path) -> Result<WatchDaemonState, StateLoadError> {
    load_watch_state_from_path(&watch_daemon_state_path(synrepo_dir))
}

/// Inspect whether the repo currently has a live, stale, or missing watch service.
pub fn watch_service_status(synrepo_dir: &std::path::Path) -> WatchServiceStatus {
    let flock_path = watch_flock_path(synrepo_dir);
    let state_path = watch_daemon_state_path(synrepo_dir);
    let state_load = load_watch_state_from_path(&state_path);

    if !flock_path.exists() {
        return match state_load {
            Ok(state) => WatchServiceStatus::Stale(Some(state)),
            Err(StateLoadError::NotFound) => WatchServiceStatus::Inactive,
            Err(StateLoadError::Malformed(e)) => WatchServiceStatus::Corrupt(e),
        };
    }

    match fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&flock_path)
    {
        Ok(file) => match file.try_lock_exclusive() {
            Ok(()) => WatchServiceStatus::Stale(state_load.ok()),
            Err(e) if crate::pipeline::writer::is_lock_contention(&e) => match state_load {
                Ok(state) => WatchServiceStatus::Running(state),
                Err(StateLoadError::NotFound) => WatchServiceStatus::Starting,
                Err(StateLoadError::Malformed(e)) => WatchServiceStatus::Corrupt(e),
            },
            Err(e) => WatchServiceStatus::Corrupt(format!("flock error: {e}")),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => WatchServiceStatus::Inactive,
        Err(e) => WatchServiceStatus::Corrupt(format!("flock open error: {e}")),
    }
}

/// Remove stale watch-service artifacts left behind by a dead process.
pub fn cleanup_stale_watch_artifacts(
    synrepo_dir: &std::path::Path,
) -> Result<bool, WatchDaemonError> {
    match watch_service_status(synrepo_dir) {
        WatchServiceStatus::Running(_) | WatchServiceStatus::Starting => Ok(false),
        WatchServiceStatus::Inactive => {
            remove_ignore_missing(watch_flock_path(synrepo_dir))?;
            remove_ignore_missing(watch_socket_path(synrepo_dir))
        }
        WatchServiceStatus::Stale(_) | WatchServiceStatus::Corrupt(_) => {
            remove_ignore_missing(watch_daemon_state_path(synrepo_dir))?;
            remove_ignore_missing(watch_flock_path(synrepo_dir))?;
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
