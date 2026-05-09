//! Shared changed-file discovery for continuation and changed-context surfaces.

use std::path::Path;
use std::process::Command;

/// Return Git porcelain changed paths, sorted and deduplicated.
pub fn git_changed_files(repo_root: &Path) -> anyhow::Result<Vec<String>> {
    let output = Command::new("git")
        .args(["status", "--porcelain=v1", "-z"])
        .current_dir(repo_root)
        .output()?;
    if !output.status.success() {
        return Ok(vec![]);
    }
    let mut files = Vec::new();
    let mut records = output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|r| !r.is_empty());
    while let Some(record) = records.next() {
        if record.len() < 4 {
            continue;
        }
        let status = &record[..2];
        let path = &record[3..];
        if !path.is_empty() {
            files.push(String::from_utf8_lossy(path).into_owned());
        }
        if status.iter().any(|code| matches!(*code, b'R' | b'C')) {
            let _old_path = records.next();
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
        fs::write(repo.path().join("old path.rs"), "fn old() {}\n").unwrap();
        fs::write(repo.path().join("modified path.rs"), "fn modified() {}\n").unwrap();
        git(repo.path(), &["add", "."]);
        git(repo.path(), &["commit", "-m", "init"]);

        git(repo.path(), &["mv", "old path.rs", "new path.rs"]);
        fs::write(
            repo.path().join("modified path.rs"),
            "fn modified_again() {}\n",
        )
        .unwrap();
        fs::write(
            repo.path().join("untracked quote cafe.rs"),
            "fn untracked() {}\n",
        )
        .unwrap();

        let files = git_changed_files(repo.path()).unwrap();

        assert_eq!(
            files,
            vec![
                "modified path.rs".to_string(),
                "new path.rs".to_string(),
                "untracked quote cafe.rs".to_string()
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
