use std::path::Path;

use crate::config::Config;
use serde::Serialize;

use super::super::git::{
    GitCommitSummary, GitDegradedReason, GitIntelligenceContext, GitIntelligenceReadiness,
};

/// Deterministic status snapshot for future git-intelligence consumers.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GitIntelligenceStatus {
    /// The revision that current Git-derived outputs should cite.
    pub source_revision: String,
    /// The configured history depth budget for deterministic mining work.
    pub requested_commit_depth: u32,
    /// Whether git-intelligence can proceed normally or must report degraded state.
    pub readiness: GitIntelligenceReadiness,
}

impl GitIntelligenceStatus {
    /// Inspect repository Git state through the pipeline context boundary.
    pub fn inspect(repo_root: &Path, config: &Config) -> Self {
        let context = GitIntelligenceContext::inspect(repo_root, config);
        Self::from_context(&context)
    }

    /// Build a status view from the shared git-intelligence context.
    pub fn from_context(context: &GitIntelligenceContext) -> Self {
        Self {
            source_revision: context.source_revision().to_string(),
            requested_commit_depth: context.requested_commit_depth(),
            readiness: context.readiness(),
        }
    }

    /// Return `true` when the repository state requires degraded-history qualifiers.
    pub fn is_degraded(&self) -> bool {
        matches!(self.readiness, GitIntelligenceReadiness::Degraded { .. })
    }

    /// Return the degraded-history reasons, or an empty list when fully ready.
    pub fn degraded_reasons(&self) -> &[GitDegradedReason] {
        match &self.readiness {
            GitIntelligenceReadiness::Ready => &[],
            GitIntelligenceReadiness::Degraded { reasons } => reasons.as_slice(),
        }
    }
}

/// A deterministic recent-history sample for card and routing enrichment.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GitHistorySample {
    /// Status metadata for the sampled repository state.
    pub status: GitIntelligenceStatus,
    /// Recent first-parent commit summaries, newest first.
    pub commits: Vec<GitCommitSummary>,
}

/// A frequently changed file in the sampled history window.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GitFileHotspot {
    /// Repository-relative file path.
    pub path: String,
    /// Number of sampled commits touching this path.
    pub touches: usize,
    /// Most recent sampled revision touching this path.
    pub last_revision: String,
    /// Most recent sampled summary touching this path.
    pub last_summary: String,
}

/// A deterministic ownership hint derived from sampled touch counts.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GitOwnershipHint {
    /// Repository-relative file path.
    pub path: String,
    /// Author with the highest sampled touch count for this path.
    pub primary_author: String,
    /// Number of sampled touches by the primary author.
    pub primary_author_touches: usize,
    /// Total sampled touches for the path.
    pub total_touches: usize,
}

/// A pair of paths that frequently changed together in the sampled window.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GitCoChange {
    /// Lexicographically first repository-relative path in the pair.
    pub left_path: String,
    /// Lexicographically second repository-relative path in the pair.
    pub right_path: String,
    /// Number of sampled commits changing both paths together.
    pub co_change_count: usize,
}

/// Repository-level git-intelligence insights derived from recent history.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GitHistoryInsights {
    /// Recent commit summaries for display surfaces.
    pub history: GitHistorySample,
    /// Files with the highest sampled change frequency.
    pub hotspots: Vec<GitFileHotspot>,
    /// Ownership hints derived from sampled author touches.
    pub ownership: Vec<GitOwnershipHint>,
    /// File pairs that co-changed in sampled commits.
    pub co_changes: Vec<GitCoChange>,
}

/// A frequent co-change partner for a specific path in the sampled window.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GitPathCoChangePartner {
    /// Repository-relative file path that changed alongside the target path.
    pub path: String,
    /// Number of sampled commits changing both paths together.
    pub co_change_count: usize,
}

/// Path-scoped git-intelligence derived from the sampled history window.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GitPathHistoryInsights {
    /// Repository-relative file path being analyzed.
    pub path: String,
    /// Status metadata for the sampled repository state.
    pub status: GitIntelligenceStatus,
    /// Recent sampled commits touching this path, newest first.
    pub commits: Vec<GitCommitSummary>,
    /// Hotspot summary for this path when it appeared in the sample window.
    pub hotspot: Option<GitFileHotspot>,
    /// Ownership hint for this path when it appeared in the sample window.
    pub ownership: Option<GitOwnershipHint>,
    /// Paths that most frequently changed alongside this one.
    pub co_change_partners: Vec<GitPathCoChangePartner>,
}
