//! Shared git test helpers for pipeline unit tests.

use std::process::Command;

/// Helper to execute a git command and return its structured output.
pub(crate) fn git_run(repo: &tempfile::TempDir, args: &[&str]) -> std::process::Output {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?} failed: stdout={}, stderr={}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

/// Helper to execute a git command without returning its output.
pub(crate) fn git(repo: &tempfile::TempDir, args: &[&str]) {
    git_run(repo, args);
}

/// Helper to execute a git command and return its standard output as a trimmed string.
pub(crate) fn git_stdout(repo: &tempfile::TempDir, args: &[&str]) -> String {
    String::from_utf8_lossy(&git_run(repo, args).stdout)
        .trim()
        .to_string()
}

/// Run a git command with overridden author and committer identity.
pub(crate) fn git_with_author(repo: &tempfile::TempDir, args: &[&str], author: &str, email: &str) {
    let output = Command::new("git")
        .env("GIT_AUTHOR_NAME", author)
        .env("GIT_AUTHOR_EMAIL", email)
        .env("GIT_COMMITTER_NAME", author)
        .env("GIT_COMMITTER_EMAIL", email)
        .args(args)
        .current_dir(repo.path())
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

/// Initialize a new git repository in the provided temp directory and create an initial commit.
pub(crate) fn init_commit(repo: &tempfile::TempDir) {
    std::fs::write(repo.path().join("tracked.txt"), "hello\n").unwrap();
    git(repo, &["init"]);
    git(repo, &["config", "user.name", "synrepo"]);
    git(repo, &["config", "user.email", "synrepo@example.com"]);
    git(repo, &["add", "tracked.txt"]);
    git(repo, &["commit", "-m", "initial"]);
}
