//! Project `GitPathHistoryInsights` onto card-shaped last-change payloads.

use super::types::{LastChangeGranularity, SymbolLastChange};
use crate::pipeline::{
    git::{GitCommitSummary, GitIntelligenceReadiness},
    git_intelligence::GitPathHistoryInsights,
};

/// Number of hex characters of the commit SHA included in
/// `SymbolLastChange.revision`. 12 is enough to be unambiguous in any
/// real-world repository while staying compact in cards.
const SHORT_REVISION_LEN: usize = 12;

/// Project `GitPathHistoryInsights` onto a `SymbolLastChange`.
///
/// When `last_modified_rev` is `Some(rev)`, resolves that revision's metadata
/// from the sampled history and returns `granularity: "symbol"`. Otherwise
/// falls back to the file's newest commit with `granularity: "file"`.
/// `include_summary` maps to the budget tier: `Deep` includes the summary
/// line, `Normal` omits it.
pub(crate) fn symbol_last_change_from_insights(
    insights: &GitPathHistoryInsights,
    include_summary: bool,
    last_modified_rev: Option<&str>,
) -> Option<SymbolLastChange> {
    // Try symbol-scoped resolution first.
    if let Some(target_rev) = last_modified_rev {
        if matches!(insights.status.readiness, GitIntelligenceReadiness::Ready) {
            if let Some(commit) = insights.commits.iter().find(|c| c.revision == target_rev) {
                return Some(build_last_change(
                    commit,
                    include_summary,
                    LastChangeGranularity::Symbol,
                ));
            }
        }
    }

    // File-level fallback.
    let commit = insights.commits.first()?;
    let granularity = match insights.status.readiness {
        GitIntelligenceReadiness::Ready => LastChangeGranularity::File,
        GitIntelligenceReadiness::Degraded { .. } => LastChangeGranularity::Unknown,
    };
    Some(build_last_change(commit, include_summary, granularity))
}

fn build_last_change(
    commit: &GitCommitSummary,
    include_summary: bool,
    granularity: LastChangeGranularity,
) -> SymbolLastChange {
    let short = commit
        .revision
        .get(..SHORT_REVISION_LEN)
        .unwrap_or(commit.revision.as_str())
        .to_string();
    SymbolLastChange {
        revision: short,
        summary: if include_summary {
            Some(commit.summary.clone())
        } else {
            None
        },
        author_name: commit.author_name.clone(),
        committed_at_unix: commit.committed_at_unix,
        granularity,
    }
}

#[cfg(test)]
mod tests {
    use super::{super::types::LastChangeGranularity, symbol_last_change_from_insights};
    use crate::pipeline::{
        git::{GitCommitSummary, GitDegradedReason, GitIntelligenceReadiness},
        git_intelligence::{
            GitFileHotspot, GitIntelligenceStatus, GitOwnershipHint, GitPathHistoryInsights,
        },
    };
    use crate::surface::card::git::SymbolLastChange;

    fn sample_insights(readiness: GitIntelligenceReadiness) -> GitPathHistoryInsights {
        GitPathHistoryInsights {
            path: "src/lib.rs".to_string(),
            status: GitIntelligenceStatus {
                source_revision: "0123456789abcdef0123".to_string(),
                requested_commit_depth: 8,
                readiness,
            },
            commits: vec![
                GitCommitSummary {
                    revision: "0123456789abcdef0123".to_string(),
                    summary: "rewrite lib".to_string(),
                    author_name: "Alice".to_string(),
                    committed_at_unix: 456,
                    parent_count: 1,
                },
                GitCommitSummary {
                    revision: "fedcba9876543210fedc".to_string(),
                    summary: "earlier touch".to_string(),
                    author_name: "Bob".to_string(),
                    committed_at_unix: 123,
                    parent_count: 1,
                },
            ],
            hotspot: Some(GitFileHotspot {
                path: "src/lib.rs".to_string(),
                touches: 2,
                last_revision: "0123456789abcdef0123".to_string(),
                last_summary: "rewrite lib".to_string(),
            }),
            ownership: Some(GitOwnershipHint {
                path: "src/lib.rs".to_string(),
                primary_author: "Alice".to_string(),
                primary_author_touches: 1,
                total_touches: 2,
            }),
            co_change_partners: vec![],
        }
    }

    #[test]
    fn symbol_last_change_ready_without_summary() {
        let insights = sample_insights(GitIntelligenceReadiness::Ready);
        let got = symbol_last_change_from_insights(&insights, false, None).unwrap();
        assert_eq!(
            got,
            SymbolLastChange {
                revision: "0123456789ab".to_string(),
                summary: None,
                author_name: "Alice".to_string(),
                committed_at_unix: 456,
                granularity: LastChangeGranularity::File,
            }
        );
    }

    #[test]
    fn symbol_last_change_ready_with_summary() {
        let insights = sample_insights(GitIntelligenceReadiness::Ready);
        let got = symbol_last_change_from_insights(&insights, true, None).unwrap();
        assert_eq!(got.summary.as_deref(), Some("rewrite lib"));
        assert_eq!(got.granularity, LastChangeGranularity::File);
    }

    #[test]
    fn symbol_last_change_degraded_uses_unknown_granularity() {
        let insights = sample_insights(GitIntelligenceReadiness::Degraded {
            reasons: vec![GitDegradedReason::ShallowHistory],
        });
        let got = symbol_last_change_from_insights(&insights, false, None).unwrap();
        assert_eq!(got.granularity, LastChangeGranularity::Unknown);
        assert_eq!(got.revision, "0123456789ab");
    }

    #[test]
    fn symbol_last_change_empty_commits_returns_none() {
        let mut insights = sample_insights(GitIntelligenceReadiness::Ready);
        insights.commits.clear();
        assert!(symbol_last_change_from_insights(&insights, true, None).is_none());
    }

    #[test]
    fn symbol_last_change_with_symbol_granularity_when_rev_set() {
        let insights = sample_insights(GitIntelligenceReadiness::Ready);
        // The second commit in sample_insights has revision "fedcba9876543210fedc".
        let got = symbol_last_change_from_insights(&insights, true, Some("fedcba9876543210fedc"))
            .unwrap();
        assert_eq!(got.granularity, LastChangeGranularity::Symbol);
        assert_eq!(got.author_name, "Bob");
        assert_eq!(got.summary.as_deref(), Some("earlier touch"));
    }

    #[test]
    fn symbol_last_change_file_granularity_when_rev_none() {
        let insights = sample_insights(GitIntelligenceReadiness::Ready);
        let got = symbol_last_change_from_insights(&insights, false, None).unwrap();
        assert_eq!(got.granularity, LastChangeGranularity::File);
    }

    #[test]
    fn symbol_last_change_falls_back_when_rev_not_in_commits() {
        let insights = sample_insights(GitIntelligenceReadiness::Ready);
        let got = symbol_last_change_from_insights(&insights, false, Some("nonexistent_revision"))
            .unwrap();
        // Falls back to file granularity (first commit).
        assert_eq!(got.granularity, LastChangeGranularity::File);
        assert_eq!(got.author_name, "Alice");
    }
}
