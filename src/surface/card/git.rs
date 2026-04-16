use serde::{Deserialize, Serialize};

use crate::pipeline::{
    git::{GitCommitSummary, GitIntelligenceReadiness},
    git_intelligence::{GitOwnershipHint, GitPathCoChangePartner, GitPathHistoryInsights},
};

/// Upper bound on the number of commits, co-change partners, etc. folded
/// into a file-scoped git-intelligence payload. Matches the value used by
/// `synrepo node <file_id>` so both surfaces stay in sync.
pub(crate) const FILE_NODE_GIT_INSIGHT_LIMIT: usize = 5;

/// Number of hex characters of the commit SHA included in
/// `SymbolLastChange.revision`. 12 is enough to be unambiguous in any
/// real-world repository while staying compact in cards.
const SHORT_REVISION_LEN: usize = 12;

/// A recent Git commit touching a file surface.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FileGitCommit {
    /// Hex SHA of the commit object.
    pub revision: String,
    /// Folded one-line commit summary.
    pub summary: String,
    /// Author name from the commit object.
    pub author_name: String,
    /// Committer timestamp in seconds since UNIX epoch.
    pub committed_at_unix: i64,
    /// Number of parents recorded on the commit.
    pub parent_count: usize,
}

impl From<GitCommitSummary> for FileGitCommit {
    fn from(commit: GitCommitSummary) -> Self {
        Self {
            revision: commit.revision,
            summary: commit.summary,
            author_name: commit.author_name,
            committed_at_unix: commit.committed_at_unix,
            parent_count: commit.parent_count,
        }
    }
}

/// A path that frequently changed alongside a file surface.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FileGitCoChange {
    /// Repository-relative file path.
    pub path: String,
    /// Number of sampled commits changing both paths together.
    pub co_change_count: usize,
}

impl From<GitPathCoChangePartner> for FileGitCoChange {
    fn from(partner: GitPathCoChangePartner) -> Self {
        Self {
            path: partner.path,
            co_change_count: partner.co_change_count,
        }
    }
}

/// Git-derived change context for a file-facing surface.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FileGitIntelligence {
    /// Whether sampled history is fully available or degraded.
    pub status: GitIntelligenceReadiness,
    /// Recent sampled commits touching this file, newest first.
    pub commits: Vec<FileGitCommit>,
    /// Number of sampled touches if the file appeared in the history window.
    pub hotspot_touches: Option<usize>,
    /// Most likely sampled author for this file.
    pub ownership: Option<FileGitOwnership>,
    /// Paths that most frequently changed alongside this file.
    pub co_change_partners: Vec<FileGitCoChange>,
}

impl From<GitPathHistoryInsights> for FileGitIntelligence {
    fn from(insights: GitPathHistoryInsights) -> Self {
        Self {
            status: insights.status.readiness,
            commits: insights.commits.into_iter().map(Into::into).collect(),
            hotspot_touches: insights.hotspot.map(|hotspot| hotspot.touches),
            ownership: insights.ownership.map(Into::into),
            co_change_partners: insights
                .co_change_partners
                .into_iter()
                .map(Into::into)
                .collect(),
        }
    }
}

impl From<&GitPathHistoryInsights> for FileGitIntelligence {
    fn from(insights: &GitPathHistoryInsights) -> Self {
        Self {
            status: insights.status.readiness.clone(),
            commits: insights.commits.iter().cloned().map(Into::into).collect(),
            hotspot_touches: insights.hotspot.as_ref().map(|hotspot| hotspot.touches),
            ownership: insights.ownership.clone().map(Into::into),
            co_change_partners: insights
                .co_change_partners
                .iter()
                .cloned()
                .map(Into::into)
                .collect(),
        }
    }
}

/// A file-surface ownership hint derived from sampled Git touches.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FileGitOwnership {
    /// Author with the highest sampled touch count for this file.
    pub primary_author: String,
    /// Number of sampled touches by the primary author.
    pub primary_author_touches: usize,
    /// Total sampled touches for the file.
    pub total_touches: usize,
}

impl From<GitOwnershipHint> for FileGitOwnership {
    fn from(ownership: GitOwnershipHint) -> Self {
        Self {
            primary_author: ownership.primary_author,
            primary_author_touches: ownership.primary_author_touches,
            total_touches: ownership.total_touches,
        }
    }
}

/// Precision qualifier for a `SymbolLastChange` payload.
///
/// Today all symbols in a file share the file's last-change proxy (`File`).
/// The enum is shaped so a future per-symbol tracker can flip the value to
/// `Symbol` without changing the card surface. `Unknown` is returned when
/// the underlying git history is degraded.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LastChangeGranularity {
    /// Most recent commit touching the containing file.
    File,
    /// Most recent commit modifying this symbol's body hash (future).
    Symbol,
    /// Git history is degraded; the value is a best-effort approximation.
    Unknown,
}

impl LastChangeGranularity {
    /// Stable snake_case identifier matching the serde representation.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Symbol => "symbol",
            Self::Unknown => "unknown",
        }
    }
}

/// Most-recent-change payload projected onto a `SymbolCard`.
///
/// V1 sources this from `FileGitIntelligence`: all symbols in a file share
/// the file's newest commit with `granularity = File`. The `granularity`
/// field makes that approximation explicit so downstream consumers can
/// discount it; per-symbol tracking (Option D in the design) upgrades only
/// the enum value, not the struct shape.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SymbolLastChange {
    /// Short hex prefix of the commit SHA.
    pub revision: String,
    /// Folded one-line summary of the commit. Omitted below `Deep` budget.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Author name from the commit object.
    pub author_name: String,
    /// Committer timestamp in seconds since UNIX epoch.
    pub committed_at_unix: i64,
    /// Precision qualifier for this payload.
    pub granularity: LastChangeGranularity,
}

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
    use super::{
        symbol_last_change_from_insights, FileGitCoChange, FileGitCommit, FileGitIntelligence,
        FileGitOwnership, LastChangeGranularity, SymbolLastChange,
    };
    use crate::pipeline::{
        git::{GitCommitSummary, GitDegradedReason, GitIntelligenceReadiness},
        git_intelligence::{
            GitFileHotspot, GitIntelligenceStatus, GitOwnershipHint, GitPathCoChangePartner,
            GitPathHistoryInsights,
        },
    };

    #[test]
    fn file_git_intelligence_converts_from_path_history_insights() {
        let projection = FileGitIntelligence::from(GitPathHistoryInsights {
            path: "src/lib.rs".to_string(),
            status: GitIntelligenceStatus {
                source_revision: "deadbeef".to_string(),
                requested_commit_depth: 8,
                readiness: GitIntelligenceReadiness::Ready,
            },
            commits: vec![GitCommitSummary {
                revision: "deadbeef".to_string(),
                summary: "touch lib".to_string(),
                author_name: "Alice".to_string(),
                committed_at_unix: 123,
                parent_count: 1,
            }],
            hotspot: Some(GitFileHotspot {
                path: "src/lib.rs".to_string(),
                touches: 3,
                last_revision: "deadbeef".to_string(),
                last_summary: "touch lib".to_string(),
            }),
            ownership: Some(GitOwnershipHint {
                path: "src/lib.rs".to_string(),
                primary_author: "Alice".to_string(),
                primary_author_touches: 2,
                total_touches: 3,
            }),
            co_change_partners: vec![GitPathCoChangePartner {
                path: "src/helper.rs".to_string(),
                co_change_count: 2,
            }],
        });

        assert_eq!(projection.status, GitIntelligenceReadiness::Ready);
        assert_eq!(
            projection.commits,
            vec![FileGitCommit {
                revision: "deadbeef".to_string(),
                summary: "touch lib".to_string(),
                author_name: "Alice".to_string(),
                committed_at_unix: 123,
                parent_count: 1,
            }]
        );
        assert_eq!(projection.hotspot_touches, Some(3));
        assert_eq!(
            projection.ownership,
            Some(FileGitOwnership {
                primary_author: "Alice".to_string(),
                primary_author_touches: 2,
                total_touches: 3,
            })
        );
        assert_eq!(
            projection.co_change_partners,
            vec![FileGitCoChange {
                path: "src/helper.rs".to_string(),
                co_change_count: 2,
            }]
        );
    }

    #[test]
    fn last_change_granularity_stable_identifiers() {
        assert_eq!(LastChangeGranularity::File.as_str(), "file");
        assert_eq!(LastChangeGranularity::Symbol.as_str(), "symbol");
        assert_eq!(LastChangeGranularity::Unknown.as_str(), "unknown");

        // serde must produce the same snake_case strings so the wire format
        // is stable across changes to the enum.
        assert_eq!(
            serde_json::to_string(&LastChangeGranularity::File).unwrap(),
            "\"file\""
        );
        assert_eq!(
            serde_json::to_string(&LastChangeGranularity::Symbol).unwrap(),
            "\"symbol\""
        );
        assert_eq!(
            serde_json::to_string(&LastChangeGranularity::Unknown).unwrap(),
            "\"unknown\""
        );
    }

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
