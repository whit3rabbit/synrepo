//! Shared changed-file discovery for continuation and changed-context surfaces.

use std::path::Path;
use std::process::Command;

/// Return Git porcelain changed paths, sorted and deduplicated.
pub fn git_changed_files(repo_root: &Path) -> anyhow::Result<Vec<String>> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(repo_root)
        .output()?;
    if !output.status.success() {
        return Ok(vec![]);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut files = Vec::new();
    for line in stdout.lines() {
        if line.len() < 4 {
            continue;
        }
        let path = line[3..].trim();
        let path = path.rsplit(" -> ").next().unwrap_or(path);
        if !path.is_empty() {
            files.push(path.to_string());
        }
    }
    files.sort();
    files.dedup();
    Ok(files)
}

#[cfg(test)]
mod tests {
    use std::{fs, process::Command};

    use tempfile::tempdir;

    use super::git_changed_files;

    #[test]
    fn git_changed_files_reports_modified_untracked_and_renamed_paths() {
        let repo = tempdir().unwrap();
        git(repo.path(), &["init", "-b", "main"]);
        git(repo.path(), &["config", "user.email", "test@example.com"]);
        git(repo.path(), &["config", "user.name", "Test"]);
        fs::write(repo.path().join("old.rs"), "fn old() {}\n").unwrap();
        fs::write(repo.path().join("modified.rs"), "fn modified() {}\n").unwrap();
        git(repo.path(), &["add", "."]);
        git(repo.path(), &["commit", "-m", "init"]);

        git(repo.path(), &["mv", "old.rs", "new.rs"]);
        fs::write(repo.path().join("modified.rs"), "fn modified_again() {}\n").unwrap();
        fs::write(repo.path().join("untracked.rs"), "fn untracked() {}\n").unwrap();

        let files = git_changed_files(repo.path()).unwrap();

        assert_eq!(
            files,
            vec![
                "modified.rs".to_string(),
                "new.rs".to_string(),
                "untracked.rs".to_string()
            ]
        );
    }

    fn git(repo: &std::path::Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(repo)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {:?} failed: stdout={}, stderr={}",
            args,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
