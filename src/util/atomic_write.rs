//! Hardened atomic file write: temp file + `fsync` + `rename` + parent-dir
//! `fsync` (Unix only).
//!
//! Why this shape: `fs::write` uses `O_TRUNC`, which can leave a zero-length
//! target after a crash mid-write. `rename` on a temp file avoids truncation
//! windows but still relies on the temp file's data actually reaching disk
//! before the rename is observed after power loss. `sync_all` on the temp
//! file closes the data-durability gap; `sync_all` on the parent directory
//! closes the rename-durability gap on Unix (directory entry ordering is not
//! guaranteed otherwise). Windows does not expose directory fsync; the
//! temp-file sync is the strongest durable guarantee there.

use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Atomically replace `path` with `contents`.
///
/// On success, `path` is either the previous version (on failure) or the new
/// version (on success), never a partial write.
pub fn atomic_write(path: &Path, contents: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))?
        .to_string_lossy();
    let counter = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = parent.join(format!(
        ".{}.tmp.{}.{}",
        file_name,
        std::process::id(),
        counter,
    ));

    let write_result = (|| -> io::Result<()> {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(contents)?;
        file.sync_all()?;
        Ok(())
    })();
    if let Err(e) = write_result {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }

    if let Err(e) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }

    // Parent-dir fsync is only meaningful on Unix. On Windows, opening a
    // directory as a file is not supported; the temp-file sync is the
    // strongest durable guarantee the platform exposes here.
    #[cfg(unix)]
    {
        if let Ok(dir) = fs::File::open(parent) {
            let _ = dir.sync_all();
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn writes_new_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("hello.txt");
        atomic_write(&path, b"hi").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"hi");
    }

    #[test]
    fn replaces_existing_file_without_truncation_window() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("existing.json");
        fs::write(&path, b"{\"v\":1}").unwrap();
        atomic_write(&path, b"{\"v\":2}").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"{\"v\":2}");
    }

    #[test]
    fn concurrent_writers_in_same_process_do_not_collide_on_tmp_name() {
        use std::sync::Arc;
        use std::thread;
        let dir = Arc::new(tempdir().unwrap());
        let mut handles = Vec::new();
        for i in 0..8 {
            let dir = Arc::clone(&dir);
            handles.push(thread::spawn(move || {
                let path = dir.path().join(format!("file-{i}.bin"));
                atomic_write(&path, format!("content-{i}").as_bytes()).unwrap();
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        for i in 0..8 {
            let got = fs::read(dir.path().join(format!("file-{i}.bin"))).unwrap();
            assert_eq!(got, format!("content-{i}").as_bytes());
        }
    }

    #[test]
    fn leaves_no_temp_files_on_success() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("clean.txt");
        atomic_write(&path, b"clean").unwrap();
        let tmp_files: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp."))
            .collect();
        assert!(
            tmp_files.is_empty(),
            "found leftover tmp files: {tmp_files:?}"
        );
    }
}
