//! Shared git test helpers for pipeline unit tests.

use std::process::Command;

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

pub(crate) fn git(repo: &tempfile::TempDir, args: &[&str]) {
    git_run(repo, args);
}

pub(crate) fn git_stdout(repo: &tempfile::TempDir, args: &[&str]) -> String {
    String::from_utf8_lossy(&git_run(repo, args).stdout)
        .trim()
        .to_string()
}

pub(crate) fn init_commit(repo: &tempfile::TempDir) {
    std::fs::write(repo.path().join("tracked.txt"), "hello\n").unwrap();
    git(repo, &["init"]);
    git(repo, &["config", "user.name", "synrepo"]);
    git(repo, &["config", "user.email", "synrepo@example.com"]);
    git(repo, &["add", "tracked.txt"]);
    git(repo, &["commit", "-m", "initial"]);
}
