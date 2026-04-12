use crate::{
    config::Config,
    pipeline::git::{
        test_support::{git, git_stdout, git_with_author, init_commit},
        GitCommitSummary, GitIntelligenceContext, GitIntelligenceReadiness,
    },
    pipeline::git_intelligence::{
        analyze_path_history, GitFileHotspot, GitIntelligenceStatus, GitOwnershipHint,
        GitPathCoChangePartner, GitPathHistoryInsights,
    },
};
use tempfile::tempdir;

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
