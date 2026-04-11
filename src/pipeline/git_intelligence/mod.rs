//! Deterministic, non-canonical Git-intelligence preparation.
//!
//! This module is the intended entry point for future history-mining work.
//! It consumes the typed pipeline Git context instead of opening `gix`
//! directly, which keeps degraded-history handling and config coupling in one place.

use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
};

use crate::config::Config;
use serde::Serialize;

use super::git::{
    GitCommitChangeSet, GitCommitSummary, GitDegradedReason, GitIntelligenceContext,
    GitIntelligenceReadiness,
};

#[cfg(test)]
mod tests;

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

/// Sample recent first-parent history through the git-intelligence boundary.
pub fn sample_recent_history(
    context: &GitIntelligenceContext,
    max_commits: usize,
) -> crate::Result<GitHistorySample> {
    Ok(GitHistorySample {
        status: GitIntelligenceStatus::from_context(context),
        commits: context.recent_first_parent_commits(max_commits)?,
    })
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

/// Derive deterministic hotspot, ownership, and co-change summaries from recent history.
pub fn analyze_recent_history(
    context: &GitIntelligenceContext,
    max_commits: usize,
    max_results: usize,
) -> crate::Result<GitHistoryInsights> {
    let change_sets = context.recent_first_parent_commit_changes(max_commits)?;
    let history = GitHistorySample {
        status: GitIntelligenceStatus::from_context(context),
        commits: change_sets.iter().map(|cs| cs.commit.clone()).collect(),
    };

    let mut file_touches: HashMap<String, (usize, String, String)> = HashMap::new();
    let mut ownership_counts: HashMap<String, BTreeMap<String, usize>> = HashMap::new();
    let mut co_change_counts: BTreeMap<(String, String), usize> = BTreeMap::new();

    for GitCommitChangeSet {
        commit,
        changed_paths,
    } in &change_sets
    {
        for path in changed_paths {
            let entry = file_touches
                .entry(path.clone())
                .or_insert_with(|| (0, commit.revision.clone(), commit.summary.clone()));
            entry.0 += 1;
            *ownership_counts
                .entry(path.clone())
                .or_default()
                .entry(commit.author_name.clone())
                .or_insert(0) += 1;
        }

        for (index, left) in changed_paths.iter().enumerate() {
            for right in changed_paths.iter().skip(index + 1) {
                let pair = if left <= right {
                    (left.clone(), right.clone())
                } else {
                    (right.clone(), left.clone())
                };
                *co_change_counts.entry(pair).or_insert(0) += 1;
            }
        }
    }

    let mut hotspots: Vec<_> = file_touches
        .into_iter()
        .map(
            |(path, (touches, last_revision, last_summary))| GitFileHotspot {
                path,
                touches,
                last_revision,
                last_summary,
            },
        )
        .collect();
    hotspots.sort_by(|left, right| {
        right
            .touches
            .cmp(&left.touches)
            .then_with(|| left.path.cmp(&right.path))
    });
    hotspots.truncate(max_results);

    let mut ownership: Vec<_> = ownership_counts
        .into_iter()
        .map(|(path, authors)| {
            let total_touches = authors.values().sum();
            let (primary_author, primary_author_touches) = authors
                .into_iter()
                .max_by(|left, right| left.1.cmp(&right.1).then_with(|| right.0.cmp(&left.0)))
                .unwrap_or_else(|| (String::new(), 0));
            GitOwnershipHint {
                path,
                primary_author,
                primary_author_touches,
                total_touches,
            }
        })
        .collect();
    ownership.sort_by(|left, right| {
        right
            .primary_author_touches
            .cmp(&left.primary_author_touches)
            .then_with(|| left.path.cmp(&right.path))
    });
    ownership.truncate(max_results);

    let mut co_changes: Vec<_> = co_change_counts
        .into_iter()
        .map(|((left_path, right_path), co_change_count)| GitCoChange {
            left_path,
            right_path,
            co_change_count,
        })
        .collect();
    co_changes.sort_by(|left, right| {
        right
            .co_change_count
            .cmp(&left.co_change_count)
            .then_with(|| left.left_path.cmp(&right.left_path))
            .then_with(|| left.right_path.cmp(&right.right_path))
    });
    co_changes.truncate(max_results);

    Ok(GitHistoryInsights {
        history,
        hotspots,
        ownership,
        co_changes,
    })
}

/// Derive deterministic history, ownership, and co-change summaries for one path.
pub fn analyze_path_history(
    context: &GitIntelligenceContext,
    target_path: &str,
    max_commits: usize,
    max_results: usize,
) -> crate::Result<GitPathHistoryInsights> {
    let status = GitIntelligenceStatus::from_context(context);
    let change_sets = context.recent_first_parent_commit_changes(max_commits)?;

    let mut commits = Vec::new();
    let mut ownership_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut co_change_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut touches = 0usize;
    let mut last_touch: Option<(String, String)> = None;

    for GitCommitChangeSet {
        commit,
        changed_paths,
    } in &change_sets
    {
        if !changed_paths.iter().any(|path| path == target_path) {
            continue;
        }

        touches += 1;
        if last_touch.is_none() {
            last_touch = Some((commit.revision.clone(), commit.summary.clone()));
        }
        commits.push(commit.clone());
        *ownership_counts
            .entry(commit.author_name.clone())
            .or_insert(0) += 1;

        for path in changed_paths {
            if path != target_path {
                *co_change_counts.entry(path.clone()).or_insert(0) += 1;
            }
        }
    }

    commits.truncate(max_results);

    let hotspot = last_touch.map(|(last_revision, last_summary)| GitFileHotspot {
        path: target_path.to_string(),
        touches,
        last_revision,
        last_summary,
    });

    let ownership = ownership_counts
        .into_iter()
        .max_by(|left, right| left.1.cmp(&right.1).then_with(|| right.0.cmp(&left.0)))
        .map(
            |(primary_author, primary_author_touches)| GitOwnershipHint {
                path: target_path.to_string(),
                primary_author,
                primary_author_touches,
                total_touches: touches,
            },
        );

    let mut co_change_partners: Vec<_> = co_change_counts
        .into_iter()
        .map(|(path, co_change_count)| GitPathCoChangePartner {
            path,
            co_change_count,
        })
        .collect();
    co_change_partners.sort_by(|left, right| {
        right
            .co_change_count
            .cmp(&left.co_change_count)
            .then_with(|| left.path.cmp(&right.path))
    });
    co_change_partners.truncate(max_results);

    Ok(GitPathHistoryInsights {
        path: target_path.to_string(),
        status,
        commits,
        hotspot,
        ownership,
        co_change_partners,
    })
}
