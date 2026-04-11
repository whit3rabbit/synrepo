use super::{
    test_support::{git, git_stdout, init_commit},
    GitCommitSummary, GitDegradedReason, GitHeadState, GitIntelligenceContext,
    GitIntelligenceReadiness, GitRepositorySnapshot,
};
use crate::config::Config;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn snapshot_reports_unavailable_when_no_repository_exists() {
    let repo = tempdir().unwrap();

    let snapshot = GitRepositorySnapshot::inspect(repo.path());

    assert_eq!(snapshot.head(), &GitHeadState::Unavailable);
    assert!(snapshot.is_degraded());
    assert_eq!(snapshot.source_revision(), "unknown");
    assert_eq!(
        snapshot.degraded_reasons(),
        vec![GitDegradedReason::RepositoryUnavailable]
    );
}

#[test]
fn snapshot_reports_unborn_head() {
    let repo = tempdir().unwrap();
    git(&repo, &["init"]);

    let snapshot = GitRepositorySnapshot::inspect(repo.path());

    assert_eq!(snapshot.head(), &GitHeadState::Unborn);
    assert!(snapshot.is_degraded());
    assert_eq!(snapshot.source_revision(), "unknown");
    assert_eq!(
        snapshot.degraded_reasons(),
        vec![GitDegradedReason::UnbornHead]
    );
}

#[test]
fn snapshot_resolves_detached_head_revision() {
    let repo = tempdir().unwrap();
    init_commit(&repo);
    let expected = git_stdout(&repo, &["rev-parse", "HEAD"]);
    git(&repo, &["checkout", "--detach", "HEAD"]);

    let snapshot = GitRepositorySnapshot::inspect(repo.path());

    assert_eq!(
        snapshot.head(),
        &GitHeadState::Detached {
            revision: expected.clone(),
        }
    );
    assert!(snapshot.is_degraded());
    assert_eq!(snapshot.source_revision(), expected);
    assert_eq!(
        snapshot.degraded_reasons(),
        vec![GitDegradedReason::DetachedHead]
    );
}

#[test]
fn snapshot_marks_shallow_history_as_degraded() {
    let source = tempdir().unwrap();
    init_commit(&source);
    std::fs::write(source.path().join("tracked.txt"), "second\n").unwrap();
    git(&source, &["add", "tracked.txt"]);
    git(&source, &["commit", "-m", "second"]);

    let clone_parent = tempdir().unwrap();
    let clone_path = clone_parent.path().join("clone");
    let source_url = format!("file://{}", source.path().display());
    let output = Command::new("git")
        .args([
            "clone",
            "--depth",
            "1",
            &source_url,
            clone_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git clone failed: stdout={}, stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let snapshot = GitRepositorySnapshot::inspect(&clone_path);

    assert!(matches!(snapshot.head(), GitHeadState::Attached { .. }));
    assert!(snapshot.is_shallow());
    assert!(snapshot.is_degraded());
    assert_eq!(
        snapshot.degraded_reasons(),
        vec![GitDegradedReason::ShallowHistory]
    );
}

#[test]
fn intelligence_context_reports_ready_history_and_depth_budget() {
    let repo = tempdir().unwrap();
    init_commit(&repo);
    let expected = git_stdout(&repo, &["rev-parse", "HEAD"]);

    let context = GitIntelligenceContext::inspect(repo.path(), &Config::default());

    assert_eq!(context.source_revision(), expected);
    assert_eq!(
        context.requested_commit_depth(),
        Config::default().git_commit_depth
    );
    assert!(matches!(
        context.repository().head(),
        GitHeadState::Attached { .. }
    ));
    assert_eq!(context.readiness(), GitIntelligenceReadiness::Ready);
}

#[test]
fn intelligence_context_carries_degraded_snapshot_state() {
    let repo = tempdir().unwrap();
    git(&repo, &["init"]);
    let config = Config {
        git_commit_depth: 42,
        ..Config::default()
    };

    let context = GitIntelligenceContext::inspect(repo.path(), &config);

    assert_eq!(context.source_revision(), "unknown");
    assert_eq!(context.requested_commit_depth(), 42);
    assert_eq!(
        context.readiness(),
        GitIntelligenceReadiness::Degraded {
            reasons: vec![GitDegradedReason::UnbornHead],
        }
    );
}

#[test]
fn intelligence_context_collects_recent_first_parent_commit_summaries() {
    let repo = tempdir().unwrap();
    init_commit(&repo);
    std::fs::write(repo.path().join("tracked.txt"), "second\n").unwrap();
    git(&repo, &["add", "tracked.txt"]);
    git(&repo, &["commit", "-m", "second change"]);

    let context = GitIntelligenceContext::inspect(repo.path(), &Config::default());
    let commits = context.recent_first_parent_commits(8).unwrap();

    assert_eq!(commits.len(), 2);
    assert_eq!(
        commits[0],
        GitCommitSummary {
            revision: git_stdout(&repo, &["rev-parse", "HEAD"]),
            summary: "second change".to_string(),
            author_name: "synrepo".to_string(),
            committed_at_unix: commits[0].committed_at_unix,
            parent_count: 1,
        }
    );
    assert_eq!(commits[1].summary, "initial");
    assert_eq!(commits[1].parent_count, 0);
}
