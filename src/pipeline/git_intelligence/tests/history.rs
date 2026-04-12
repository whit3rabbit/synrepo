use crate::{
    config::Config,
    pipeline::git::{
        test_support::{git, git_stdout, git_with_author, init_commit},
        GitCommitSummary, GitDegradedReason, GitIntelligenceContext, GitIntelligenceReadiness,
    },
    pipeline::git_intelligence::{
        analyze_recent_history, sample_recent_history, GitCoChange, GitFileHotspot,
        GitHistorySample, GitIntelligenceStatus, GitOwnershipHint,
    },
};
use tempfile::tempdir;

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
