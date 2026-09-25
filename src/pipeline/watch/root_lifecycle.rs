use std::{
    path::Path,
    sync::{atomic::AtomicBool, Arc},
};

use super::lease::WatchServiceMode;

pub(super) fn identify_root(path: &Path) -> crate::Result<same_file::Handle> {
    same_file::Handle::from_path(path).map_err(|error| {
        crate::Error::Other(anyhow::anyhow!("cannot identify watch root: {error}"))
    })
}

pub(super) fn root_unchanged(path: &Path, original: &same_file::Handle) -> bool {
    same_file::Handle::from_path(path).ok().as_ref() == Some(original)
}

pub(super) fn register_daemon_stop_signal(
    mode: WatchServiceMode,
    stop_flag: Arc<AtomicBool>,
) -> crate::Result<()> {
    #[cfg(unix)]
    if mode == WatchServiceMode::Daemon {
        signal_hook::flag::register(signal_hook::consts::SIGTERM, stop_flag).map_err(|error| {
            crate::Error::Other(anyhow::anyhow!("cannot install watch stop signal: {error}"))
        })?;
    }
    #[cfg(not(unix))]
    let _ = (mode, stop_flag);
    Ok(())
}
