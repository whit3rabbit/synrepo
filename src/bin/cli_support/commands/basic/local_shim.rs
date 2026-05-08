use std::{
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) fn reject_parent_symlinks(repo_root: &Path, out_path: &Path) -> anyhow::Result<()> {
    let relative = out_path.strip_prefix(repo_root).map_err(|_| {
        anyhow::anyhow!(
            "shim path must stay under repo root: {}",
            out_path.display()
        )
    })?;
    let Some(parent) = relative.parent() else {
        return Ok(());
    };
    let mut current = repo_root.to_path_buf();
    for component in parent.components() {
        match component {
            Component::Normal(part) => current.push(part),
            Component::CurDir => continue,
            Component::ParentDir | Component::Prefix(_) | Component::RootDir => {
                anyhow::bail!(
                    "shim path must stay under repo root: {}",
                    out_path.display()
                )
            }
        }
        match fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                anyhow::bail!(
                    "refusing to write shim through symlink at {}",
                    current.display()
                );
            }
            Ok(meta) if !meta.is_dir() => {
                anyhow::bail!(
                    "shim parent component is not a directory: {}",
                    current.display()
                );
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => {
                return Err(anyhow::anyhow!(
                    "could not inspect {}: {error}",
                    current.display()
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn write_atomic(out_path: &Path, content: &str) -> std::io::Result<()> {
    let parent = out_path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = out_path
        .file_name()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "missing file name"))?
        .to_string_lossy();
    let mut tmp_path = temp_path(parent, &file_name);
    for attempt in 0..8 {
        if attempt > 0 {
            tmp_path = temp_path(parent, &format!("{file_name}.{attempt}"));
        }
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)
        {
            Ok(mut file) => {
                let result = (|| {
                    file.write_all(content.as_bytes())?;
                    file.sync_all()?;
                    drop(file);
                    rename_temp_over_target(&tmp_path, out_path)
                })();
                if result.is_err() {
                    let _ = fs::remove_file(&tmp_path);
                }
                return result;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not allocate a unique shim temp file",
    ))
}

fn temp_path(parent: &Path, file_name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    parent.join(format!(".{file_name}.{}.{}.tmp", std::process::id(), nanos))
}

fn rename_temp_over_target(tmp_path: &Path, out_path: &Path) -> std::io::Result<()> {
    match fs::rename(tmp_path, out_path) {
        Ok(()) => Ok(()),
        Err(error)
            if cfg!(windows)
                && matches!(
                    error.kind(),
                    std::io::ErrorKind::AlreadyExists | std::io::ErrorKind::PermissionDenied
                ) =>
        {
            fs::remove_file(out_path)?;
            fs::rename(tmp_path, out_path)
        }
        Err(error) => Err(error),
    }
}
