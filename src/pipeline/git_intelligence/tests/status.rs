use crate::{
    config::Config,
    pipeline::git::{
        test_support::{git, git_stdout, init_commit},
        GitDegradedReason, GitIntelligenceReadiness,
    },
    pipeline::git_intelligence::GitIntelligenceStatus,
};
use tempfile::tempdir;

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
