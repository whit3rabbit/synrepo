//! Optional maintenance for a standalone `.syntext/` index.

use std::{
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    time::{Duration, Instant},
};

use crate::util::atomic_write;

/// Repository-local directory used by the standalone syntext CLI.
pub const EXTERNAL_SYNTEXT_DIR: &str = ".syntext";
/// Repository-root `.gitignore` entry for the standalone syntext index.
pub const EXTERNAL_SYNTEXT_GITIGNORE_ENTRY: &str = ".syntext/";
/// Manifest file that marks a materialized standalone syntext index.
pub const EXTERNAL_SYNTEXT_MANIFEST: &str = "manifest.json";
/// Default timeout for external `st` commands invoked by synrepo.
pub const DEFAULT_ST_TIMEOUT: Duration = Duration::from_secs(30);

const ST_POLL_INTERVAL: Duration = Duration::from_millis(20);

/// Outcome for optional external `.syntext` maintenance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExternalSyntextSync {
    /// No existing external syntext index was present.
    Skipped,
    /// The external syntext updater completed successfully.
    Updated,
}

enum StCommand {
    Index,
    Update,
    Version,
}

impl StCommand {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Index => "index",
            Self::Update => "update",
            Self::Version => "--version",
        }
    }
}

/// Return the standalone syntext index directory for `repo_root`.
pub fn external_syntext_index_dir(repo_root: &Path) -> PathBuf {
    repo_root.join(EXTERNAL_SYNTEXT_DIR)
}

/// Return true when this repo already has a standalone `.syntext` index.
pub fn external_syntext_index_exists(repo_root: &Path) -> bool {
    external_syntext_index_dir(repo_root)
        .join(EXTERNAL_SYNTEXT_MANIFEST)
        .is_file()
}

/// Return true when the root `.gitignore` already ignores `.syntext/`.
pub fn root_gitignore_contains_syntext(repo_root: &Path) -> crate::Result<bool> {
    root_gitignore_contains_syntext_line(repo_root)
}

/// Append `.syntext/` to the repository root `.gitignore` if needed.
pub fn ensure_root_gitignore_entry(repo_root: &Path) -> crate::Result<bool> {
    ensure_root_syntext_gitignore_entry(repo_root)
}

/// Return true when `st` can be launched successfully from PATH.
pub fn st_available() -> bool {
    st_available_with_program(Path::new("st"), DEFAULT_ST_TIMEOUT)
}

/// Return true when `program --version` exits successfully within `timeout`.
pub fn st_available_with_program(program: &Path, timeout: Duration) -> bool {
    run_st_command(program, StCommand::Version, None, None, timeout).is_ok()
}

/// Build the standalone `.syntext/` index with `st index`.
pub fn build_external_syntext_index(repo_root: &Path) -> crate::Result<()> {
    build_external_syntext_index_with_program(repo_root, Path::new("st"), DEFAULT_ST_TIMEOUT)
}

/// Build the standalone `.syntext/` index with an explicit `st` binary.
pub fn build_external_syntext_index_with_program(
    repo_root: &Path,
    program: &Path,
    timeout: Duration,
) -> crate::Result<()> {
    let index_dir = external_syntext_index_dir(repo_root);
    run_st_command(
        program,
        StCommand::Index,
        Some(repo_root),
        Some(&index_dir),
        timeout,
    )
}

/// Refresh an existing external syntext index with `st update`.
pub fn sync_external_syntext_index(repo_root: &Path) -> crate::Result<ExternalSyntextSync> {
    sync_external_syntext_index_with_program(repo_root, Path::new("st"), DEFAULT_ST_TIMEOUT)
}

/// Refresh an existing external syntext index with an explicit `st` binary.
pub fn sync_external_syntext_index_with_program(
    repo_root: &Path,
    program: &Path,
    timeout: Duration,
) -> crate::Result<ExternalSyntextSync> {
    if !external_syntext_index_exists(repo_root) {
        return Ok(ExternalSyntextSync::Skipped);
    }

    let index_dir = external_syntext_index_dir(repo_root);
    run_st_command(
        program,
        StCommand::Update,
        Some(repo_root),
        Some(&index_dir),
        timeout,
    )?;
    Ok(ExternalSyntextSync::Updated)
}

fn run_st_command(
    program: &Path,
    command: StCommand,
    repo_root: Option<&Path>,
    index_dir: Option<&Path>,
    timeout: Duration,
) -> crate::Result<()> {
    let command_name = command.as_str();
    let mut cmd = Command::new(program);
    cmd.arg(command_name)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(repo_root) = repo_root {
        cmd.arg("--quiet").arg("--repo-root").arg(repo_root);
    }
    if let Some(index_dir) = index_dir {
        cmd.arg("--index-dir").arg(index_dir);
    }

    let mut child = cmd.spawn().map_err(|err| {
        crate::Error::Other(anyhow::anyhow!(
            "unable to run external syntext command `{}` `{command_name}`: {err}",
            program.display()
        ))
    })?;

    wait_for_st_command(&mut child, command_name, timeout)
}

fn wait_for_st_command(
    child: &mut std::process::Child,
    command_name: &str,
    timeout: Duration,
) -> crate::Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => return Err(command_failed(command_name, status)),
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(crate::Error::Other(anyhow::anyhow!(
                    "external syntext command `{command_name}` timed out after {timeout:?}"
                )));
            }
            Ok(None) => std::thread::sleep(ST_POLL_INTERVAL),
            Err(err) => return Err(err.into()),
        }
    }
}

fn command_failed(command_name: &str, status: ExitStatus) -> crate::Error {
    crate::Error::Other(anyhow::anyhow!(
        "external syntext command `{command_name}` exited with {status}"
    ))
}

fn ensure_root_syntext_gitignore_entry(repo_root: &Path) -> crate::Result<bool> {
    let gitignore_path = repo_root.join(".gitignore");
    if gitignore_path.exists() {
        let content = std::fs::read_to_string(&gitignore_path)?;
        if content.lines().any(syntext_gitignore_line) {
            return Ok(false);
        }
        let mut new_content = content;
        if !new_content.ends_with('\n') {
            new_content.push('\n');
        }
        new_content.push_str(EXTERNAL_SYNTEXT_GITIGNORE_ENTRY);
        new_content.push('\n');
        atomic_write(&gitignore_path, new_content.as_bytes())?;
    } else {
        atomic_write(
            &gitignore_path,
            format!("{EXTERNAL_SYNTEXT_GITIGNORE_ENTRY}\n").as_bytes(),
        )?;
    }
    Ok(true)
}

fn root_gitignore_contains_syntext_line(repo_root: &Path) -> crate::Result<bool> {
    let gitignore_path = repo_root.join(".gitignore");
    if !gitignore_path.exists() {
        return Ok(false);
    }
    let content = std::fs::read_to_string(gitignore_path)?;
    Ok(content.lines().any(syntext_gitignore_line))
}

fn syntext_gitignore_line(line: &str) -> bool {
    matches!(
        line.trim(),
        ".syntext/" | ".syntext" | "/.syntext/" | "/.syntext" | ".syntext/**" | "/.syntext/**"
    )
}

#[cfg(test)]
mod tests;
