//! Reverse index over recent first-parent commit changes.
//!
//! Bulk callers (export, card compile) previously ran one
//! `recent_first_parent_commit_changes(depth)` walk per queried path. For N
//! paths that is O(N × depth) work with per-commit tree diffs.
//!
//! `GitHistoryIndex::build` runs the walk **once** and builds a `path →
//! [commit_index]` map. `GitHistoryIndex::project_path` then derives the
//! same `GitPathHistoryInsights` the old `analyze_path_history` produced,
//! but by lookup instead of re-walking — amortising the walk across all
//! subsequent projections.

use std::collections::{BTreeMap, HashMap};

use crate::pipeline::git::{GitCommitChangeSet, GitIntelligenceContext};

use super::types::{
    GitFileHotspot, GitIntelligenceStatus, GitOwnershipHint, GitPathCoChangePartner,
    GitPathHistoryInsights,
};

/// Prebuilt reverse index of recent first-parent commit changes.
///
/// Construction cost is O(depth × files-per-commit); lookup cost is
/// O(touches-for-path). Build once per compiler instance or reconcile
/// epoch; project many times.
pub struct GitHistoryIndex {
    status: GitIntelligenceStatus,
    change_sets: Vec<GitCommitChangeSet>,
    by_path: HashMap<String, Vec<usize>>,
}

impl GitHistoryIndex {
    /// Walk first-parent history once and populate the reverse index.
    pub fn build(context: &GitIntelligenceContext, max_commits: usize) -> crate::Result<Self> {
        let status = GitIntelligenceStatus::from_context(context);
        let change_sets = context.recent_first_parent_commit_changes(max_commits)?;

        let mut by_path: HashMap<String, Vec<usize>> = HashMap::new();
        for (index, change_set) in change_sets.iter().enumerate() {
            for path in &change_set.changed_paths {
                by_path.entry(path.clone()).or_default().push(index);
            }
        }

        Ok(Self {
            status,
            change_sets,
            by_path,
        })
    }

    /// Project path-scoped insights from the prebuilt index.
    ///
    /// Matches the output of the removed walk-per-path implementation: same
    /// ordering, same truncation rules, same degraded-state handling via
    /// `status`.
    pub fn project_path(&self, target_path: &str, max_results: usize) -> GitPathHistoryInsights {
        let mut commits = Vec::new();
        let mut ownership_counts: BTreeMap<String, usize> = BTreeMap::new();
        let mut co_change_counts: BTreeMap<String, usize> = BTreeMap::new();
        let mut touches = 0usize;
        let mut last_touch: Option<(String, String)> = None;

        if let Some(indices) = self.by_path.get(target_path) {
            // `change_sets` is newest-first and `indices` preserves that
            // order (they were pushed during a sequential enumerate walk),
            // so the first match is the most recent touch.
            for &idx in indices {
                let GitCommitChangeSet {
                    commit,
                    changed_paths,
                } = &self.change_sets[idx];

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
        }

        commits.truncate(max_results);

        let hotspot = last_touch.map(|(last_revision, last_summary)| GitFileHotspot {
            path: target_path.to_string(),
            touches,
            last_revision,
            last_summary,
        });

        let ownership =
            primary_author(ownership_counts).map(|(author, author_touches)| GitOwnershipHint {
                path: target_path.to_string(),
                primary_author: author,
                primary_author_touches: author_touches,
                total_touches: touches,
            });

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

        GitPathHistoryInsights {
            path: target_path.to_string(),
            status: self.status.clone(),
            commits,
            hotspot,
            ownership,
            co_change_partners,
        }
    }
}

pub(super) fn primary_author(authors: BTreeMap<String, usize>) -> Option<(String, usize)> {
    authors
        .into_iter()
        .max_by(|left, right| left.1.cmp(&right.1).then_with(|| right.0.cmp(&left.0)))
}
