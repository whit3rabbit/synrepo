use std::sync::Mutex;

static HOME_LOCK: Mutex<()> = Mutex::new(());

pub(crate) struct HomeEnvGuard {
    _guard: std::sync::MutexGuard<'static, ()>,
}

impl HomeEnvGuard {
    pub(crate) fn new(path: &std::path::Path) -> Self {
        let guard = HOME_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        #[cfg(unix)]
        std::env::set_var("HOME", path);
        #[cfg(windows)]
        std::env::set_var("USERPROFILE", path);
        Self { _guard: guard }
    }
}

impl Drop for HomeEnvGuard {
    fn drop(&mut self) {
        #[cfg(unix)]
        std::env::remove_var("HOME");
        #[cfg(windows)]
        std::env::remove_var("USERPROFILE");
    }
}

mod codex;
mod mcp_clients;
mod misc;
mod steps;
