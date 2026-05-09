//! Small blocking file-lock helper for secondary state files.

use std::{
    fs::{self, File, OpenOptions},
    io,
    path::Path,
};

use fs2::FileExt;

/// RAII guard for an exclusive advisory file lock.
///
/// The lock is released when the guard drops. The lock file itself is left in
/// place so concurrent processes have a stable path to coordinate on.
#[derive(Debug)]
pub(crate) struct ExclusiveFileLock {
    _file: File,
}

/// Acquire a blocking exclusive lock at `path`, creating the parent directory
/// and lock file if needed.
pub(crate) fn exclusive_file_lock(path: &Path) -> io::Result<ExclusiveFileLock> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }

    let mut options = OpenOptions::new();
    options.create(true).read(true).write(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_CLOEXEC);
    }

    let file = options.open(path)?;
    file.lock_exclusive()?;
    Ok(ExclusiveFileLock { _file: file })
}
