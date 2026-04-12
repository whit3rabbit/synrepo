use serde::{Deserialize, Serialize};

use crate::pipeline::{
    git::{GitCommitSummary, GitIntelligenceReadiness},
    git_intelligence::{GitOwnershipHint, GitPathCoChangePartner, GitPathHistoryInsights},
};

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

#[cfg(test)]
mod tests {
    use super::{FileGitCoChange, FileGitCommit, FileGitIntelligence, FileGitOwnership};
    use crate::pipeline::{
        git::{GitCommitSummary, GitIntelligenceReadiness},
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
}
