use std::path::Path;

use crate::config::Config;

use super::{
    events::WatchEvent, lease::WatchServiceMode, service::run_watch_service_with_shutdown,
    WatchConfig,
};

/// Run the watch service in the current process.
pub fn run_watch_service(
    repo_root: &Path,
    config: &Config,
    watch_config: &WatchConfig,
    synrepo_dir: &Path,
    mode: WatchServiceMode,
    events: Option<crossbeam_channel::Sender<WatchEvent>>,
) -> crate::Result<()> {
    run_watch_service_with_shutdown(
        repo_root,
        config,
        watch_config,
        synrepo_dir,
        mode,
        events,
        false,
    )
}

/// Run the detached-daemon variant of the service. The process exits when
/// this returns, so shutdown uses a bounded wait for an active worker.
#[doc(hidden)]
pub fn run_watch_service_process_owned(
    repo_root: &Path,
    config: &Config,
    watch_config: &WatchConfig,
    synrepo_dir: &Path,
    mode: WatchServiceMode,
    events: Option<crossbeam_channel::Sender<WatchEvent>>,
) -> crate::Result<()> {
    run_watch_service_with_shutdown(
        repo_root,
        config,
        watch_config,
        synrepo_dir,
        mode,
        events,
        true,
    )
}
