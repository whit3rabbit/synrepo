//! Hidden helpers for cross-process test coordination.
//!
//! These are public so both library tests and binary-crate tests can use them,
//! but they are not part of the supported user-facing API surface.

use std::fs::{self, File, OpenOptions};
use std::path::PathBuf;

use fs2::FileExt;

/// RAII guard for a shared cross-process test lock.
#[doc(hidden)]
pub struct GlobalTestLock {
    file: File,
}

impl Drop for GlobalTestLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

/// Acquire a global cross-process test lock for `label`.
///
/// This is intentionally blocking: a small set of mutation-heavy tests are
/// known to interfere under parallel execution, so they serialize themselves
/// rather than forcing the entire workspace to run single-threaded.
#[doc(hidden)]
pub fn global_test_lock(label: &str) -> GlobalTestLock {
    let path = global_test_lock_path(label);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create global test-lock directory");
    }
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&path)
        .unwrap_or_else(|error| panic!("open global test lock {}: {error}", path.display()));
    file.lock_exclusive()
        .unwrap_or_else(|error| panic!("lock global test lock {}: {error}", path.display()));
    GlobalTestLock { file }
}

fn global_test_lock_path(label: &str) -> PathBuf {
    let safe = label
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => ch,
            _ => '-',
        })
        .collect::<String>();
    std::env::temp_dir().join(format!("synrepo-test-lock-{safe}.lock"))
}
