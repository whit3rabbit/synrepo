use super::{
    analyze_path_history, analyze_recent_history, sample_recent_history, GitCoChange,
    GitFileHotspot, GitHistorySample, GitIntelligenceStatus, GitOwnershipHint,
    GitPathCoChangePartner, GitPathHistoryInsights,
};
use crate::{
    config::Config,
    pipeline::git::{
        test_support::{git, git_stdout, init_commit},
        GitCommitSummary, GitDegradedReason, GitIntelligenceContext, GitIntelligenceReadiness,
    },
};
use std::process::Command;
use tempfile::tempdir;

fn git_with_author(repo: &tempfile::TempDir, args: &[&str], author: &str, email: &str) {
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

#[test]
fn status_reports_ready_history_for_attached_head() {
    let repo = tempdir().unwrap();
    init_commit(&repo);
    let expected = git_stdout(&repo, &["rev-parse", "HEAD"]);
    let config = Config {
        git_commit_depth: 77,
        ..Config::default()
    };

    let status = GitIntelligenceStatus::inspect(repo.path(), &config);

    assert_eq!(status.source_revision, expected);
    assert_eq!(status.requested_commit_depth, 77);
    assert_eq!(status.readiness, GitIntelligenceReadiness::Ready);
    assert!(!status.is_degraded());
    assert!(status.degraded_reasons().is_empty());
}

#[test]
fn status_reports_degraded_history_for_unborn_head() {
    let repo = tempdir().unwrap();
    git(&repo, &["init"]);

    let status = GitIntelligenceStatus::inspect(repo.path(), &Config::default());

    assert_eq!(status.source_revision, "unknown");
    assert!(status.is_degraded());
    assert_eq!(
        status.readiness,
        GitIntelligenceReadiness::Degraded {
            reasons: vec![GitDegradedReason::UnbornHead],
        }
    );
    assert_eq!(status.degraded_reasons(), &[GitDegradedReason::UnbornHead]);
}

#[test]
fn sample_recent_history_returns_recent_commit_summaries() {
    let repo = tempdir().unwrap();
    init_commit(&repo);
    std::fs::write(repo.path().join("tracked.txt"), "second\n").unwrap();
    git(&repo, &["add", "tracked.txt"]);
    git(&repo, &["commit", "-m", "second change"]);
    let context = GitIntelligenceContext::inspect(repo.path(), &Config::default());

    let sample = sample_recent_history(&context, 8).unwrap();

    assert_eq!(
        sample.status,
        GitIntelligenceStatus {
            source_revision: git_stdout(&repo, &["rev-parse", "HEAD"]),
            requested_commit_depth: Config::default().git_commit_depth,
            readiness: GitIntelligenceReadiness::Ready,
        }
    );
    assert_eq!(sample.commits.len(), 2);
    assert_eq!(
        sample.commits[0],
        GitCommitSummary {
            revision: sample.commits[0].revision.clone(),
            summary: "second change".to_string(),
            author_name: "synrepo".to_string(),
            committed_at_unix: sample.commits[0].committed_at_unix,
            parent_count: 1,
        }
    );
    assert_eq!(sample.commits[1].summary, "initial");
}

#[test]
fn sample_recent_history_returns_empty_commits_for_unborn_head() {
    let repo = tempdir().unwrap();
    git(&repo, &["init"]);
    let context = GitIntelligenceContext::inspect(repo.path(), &Config::default());

    let sample = sample_recent_history(&context, 8).unwrap();

    assert_eq!(
        sample,
        GitHistorySample {
            status: GitIntelligenceStatus {
                source_revision: "unknown".to_string(),
                requested_commit_depth: Config::default().git_commit_depth,
                readiness: GitIntelligenceReadiness::Degraded {
                    reasons: vec![GitDegradedReason::UnbornHead],
                },
            },
            commits: Vec::new(),
        }
    );
}

#[test]
fn analyze_recent_history_derives_hotspots_ownership_and_co_changes() {
    let repo = tempdir().unwrap();
    git(&repo, &["init"]);
    git(&repo, &["config", "user.name", "setup"]);
    git(&repo, &["config", "user.email", "setup@example.com"]);

    std::fs::create_dir_all(repo.path().join("src")).unwrap();
    std::fs::write(repo.path().join("src/a.txt"), "a1\n").unwrap();
    git(&repo, &["add", "src/a.txt"]);
    git_with_author(
        &repo,
        &["commit", "-m", "add a"],
        "Alice",
        "alice@example.com",
    );

    std::fs::write(repo.path().join("src/a.txt"), "a2\n").unwrap();
    std::fs::write(repo.path().join("src/b.txt"), "b1\n").unwrap();
    git(&repo, &["add", "src/a.txt", "src/b.txt"]);
    git_with_author(
        &repo,
        &["commit", "-m", "touch a and b"],
        "Bob",
        "bob@example.com",
    );

    std::fs::write(repo.path().join("src/b.txt"), "b2\n").unwrap();
    std::fs::write(repo.path().join("src/c.txt"), "c1\n").unwrap();
    git(&repo, &["add", "src/b.txt", "src/c.txt"]);
    git_with_author(
        &repo,
        &["commit", "-m", "touch b and c"],
        "Alice",
        "alice@example.com",
    );

    let context = GitIntelligenceContext::inspect(repo.path(), &Config::default());
    let insights = analyze_recent_history(&context, 8, 8).unwrap();

    assert_eq!(insights.history.commits.len(), 3);
    assert_eq!(
        insights.hotspots,
        vec![
            GitFileHotspot {
                path: "src/a.txt".to_string(),
                touches: 2,
                last_revision: git_stdout(&repo, &["rev-parse", "HEAD~1"]),
                last_summary: "touch a and b".to_string(),
            },
            GitFileHotspot {
                path: "src/b.txt".to_string(),
                touches: 2,
                last_revision: git_stdout(&repo, &["rev-parse", "HEAD"]),
                last_summary: "touch b and c".to_string(),
            },
            GitFileHotspot {
                path: "src/c.txt".to_string(),
                touches: 1,
                last_revision: git_stdout(&repo, &["rev-parse", "HEAD"]),
                last_summary: "touch b and c".to_string(),
            },
        ]
    );
    assert_eq!(
        insights.ownership,
        vec![
            GitOwnershipHint {
                path: "src/a.txt".to_string(),
                primary_author: "Alice".to_string(),
                primary_author_touches: 1,
                total_touches: 2,
            },
            GitOwnershipHint {
                path: "src/b.txt".to_string(),
                primary_author: "Alice".to_string(),
                primary_author_touches: 1,
                total_touches: 2,
            },
            GitOwnershipHint {
                path: "src/c.txt".to_string(),
                primary_author: "Alice".to_string(),
                primary_author_touches: 1,
                total_touches: 1,
            },
        ]
    );
    assert_eq!(
        insights.co_changes,
        vec![
            GitCoChange {
                left_path: "src/a.txt".to_string(),
                right_path: "src/b.txt".to_string(),
                co_change_count: 1,
            },
            GitCoChange {
                left_path: "src/b.txt".to_string(),
                right_path: "src/c.txt".to_string(),
                co_change_count: 1,
            },
        ]
    );
}

#[test]
fn analyze_path_history_derives_commits_ownership_and_co_change_partners() {
    let repo = tempdir().unwrap();
    git(&repo, &["init"]);
    git(&repo, &["config", "user.name", "setup"]);
    git(&repo, &["config", "user.email", "setup@example.com"]);

    std::fs::create_dir_all(repo.path().join("src")).unwrap();
    std::fs::write(repo.path().join("src/a.txt"), "a1\n").unwrap();
    git(&repo, &["add", "src/a.txt"]);
    git_with_author(
        &repo,
        &["commit", "-m", "add a"],
        "Alice",
        "alice@example.com",
    );

    std::fs::write(repo.path().join("src/a.txt"), "a2\n").unwrap();
    std::fs::write(repo.path().join("src/b.txt"), "b1\n").unwrap();
    git(&repo, &["add", "src/a.txt", "src/b.txt"]);
    git_with_author(
        &repo,
        &["commit", "-m", "touch a and b"],
        "Bob",
        "bob@example.com",
    );

    std::fs::write(repo.path().join("src/a.txt"), "a3\n").unwrap();
    std::fs::write(repo.path().join("src/c.txt"), "c1\n").unwrap();
    git(&repo, &["add", "src/a.txt", "src/c.txt"]);
    git_with_author(
        &repo,
        &["commit", "-m", "touch a and c"],
        "Alice",
        "alice@example.com",
    );

    let context = GitIntelligenceContext::inspect(repo.path(), &Config::default());
    let insights = analyze_path_history(&context, "src/a.txt", 8, 8).unwrap();

    assert_eq!(
        insights,
        GitPathHistoryInsights {
            path: "src/a.txt".to_string(),
            status: GitIntelligenceStatus {
                source_revision: git_stdout(&repo, &["rev-parse", "HEAD"]),
                requested_commit_depth: Config::default().git_commit_depth,
                readiness: GitIntelligenceReadiness::Ready,
            },
            commits: vec![
                GitCommitSummary {
                    revision: git_stdout(&repo, &["rev-parse", "HEAD"]),
                    summary: "touch a and c".to_string(),
                    author_name: "Alice".to_string(),
                    committed_at_unix: insights.commits[0].committed_at_unix,
                    parent_count: 1,
                },
                GitCommitSummary {
                    revision: git_stdout(&repo, &["rev-parse", "HEAD~1"]),
                    summary: "touch a and b".to_string(),
                    author_name: "Bob".to_string(),
                    committed_at_unix: insights.commits[1].committed_at_unix,
                    parent_count: 1,
                },
                GitCommitSummary {
                    revision: git_stdout(&repo, &["rev-parse", "HEAD~2"]),
                    summary: "add a".to_string(),
                    author_name: "Alice".to_string(),
                    committed_at_unix: insights.commits[2].committed_at_unix,
                    parent_count: 0,
                },
            ],
            hotspot: Some(GitFileHotspot {
                path: "src/a.txt".to_string(),
                touches: 3,
                last_revision: git_stdout(&repo, &["rev-parse", "HEAD"]),
                last_summary: "touch a and c".to_string(),
            }),
            ownership: Some(GitOwnershipHint {
                path: "src/a.txt".to_string(),
                primary_author: "Alice".to_string(),
                primary_author_touches: 2,
                total_touches: 3,
            }),
            co_change_partners: vec![
                GitPathCoChangePartner {
                    path: "src/b.txt".to_string(),
                    co_change_count: 1,
                },
                GitPathCoChangePartner {
                    path: "src/c.txt".to_string(),
                    co_change_count: 1,
                },
            ],
        }
    );
}

#[test]
fn analyze_path_history_returns_empty_path_insights_when_path_is_absent() {
    let repo = tempdir().unwrap();
    init_commit(&repo);
    let context = GitIntelligenceContext::inspect(repo.path(), &Config::default());

    let insights = analyze_path_history(&context, "src/missing.txt", 8, 8).unwrap();

    assert_eq!(
        insights,
        GitPathHistoryInsights {
            path: "src/missing.txt".to_string(),
            status: GitIntelligenceStatus {
                source_revision: git_stdout(&repo, &["rev-parse", "HEAD"]),
                requested_commit_depth: Config::default().git_commit_depth,
                readiness: GitIntelligenceReadiness::Ready,
            },
            commits: Vec::new(),
            hotspot: None,
            ownership: None,
            co_change_partners: Vec::new(),
        }
    );
}
