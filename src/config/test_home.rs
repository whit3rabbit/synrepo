//! RAII guard that redirects the user's home directory (as read by
//! [`super::home_dir`]) to a caller-chosen path for the lifetime of the guard,
//! restoring the prior value on drop.
//!
//! Tests that exercise `Config::load` (which merges `~/.synrepo/config.toml`
//! into the repo-local config) MUST take both this guard and the shared
//! cross-process test lock [`HOME_ENV_TEST_LOCK`] so they don't leak the
//! developer's real user-scoped credentials into assertions.
//!
//! Exposed as `pub #[doc(hidden)]` (not `pub(crate)` or `#[cfg(test)]`) so
//! bin-crate tests, which compile the library without `cfg(test)`, can also
//! take the guard. Same pattern as
//! `pipeline::writer::hold_writer_flock_with_ownership`.

use std::ffi::OsString;
use std::path::Path;
use std::sync::Mutex;

/// Shared label for `crate::test_support::global_test_lock`; all tests that
/// mutate the home-directory env var must serialize on this label.
pub const HOME_ENV_TEST_LOCK: &str = "config-home-env";

#[cfg(unix)]
const HOME_VAR: &str = "HOME";
#[cfg(windows)]
const HOME_VAR: &str = "USERPROFILE";

static HOME_ENV_MUTEX: Mutex<()> = Mutex::new(());

pub struct HomeEnvGuard {
    original: Option<OsString>,
    _thread_guard: std::sync::MutexGuard<'static, ()>,
}

impl HomeEnvGuard {
    pub fn redirect_to(path: &Path) -> Self {
        let thread_guard = HOME_ENV_MUTEX
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let original = std::env::var_os(HOME_VAR);
        std::env::set_var(HOME_VAR, path);
        Self {
            original,
            _thread_guard: thread_guard,
        }
    }
}

impl Drop for HomeEnvGuard {
    fn drop(&mut self) {
        match &self.original {
            Some(value) => std::env::set_var(HOME_VAR, value),
            None => std::env::remove_var(HOME_VAR),
        }
    }
}
