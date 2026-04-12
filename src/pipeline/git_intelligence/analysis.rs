use std::collections::{BTreeMap, HashMap};

use crate::pipeline::git::{GitCommitChangeSet, GitCommitSummary, GitIntelligenceContext};

use super::types::{
    GitCoChange, GitFileHotspot, GitHistoryInsights, GitHistorySample, GitIntelligenceStatus,
    GitOwnershipHint, GitPathCoChangePartner, GitPathHistoryInsights,
};

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
        record_change_set(
            commit,
            changed_paths,
            &mut file_touches,
            &mut ownership_counts,
        );
        record_co_changes(changed_paths, &mut co_change_counts);
    }

    let mut hotspots = build_hotspots(file_touches);
    hotspots.sort_by(|left, right| {
        right
            .touches
            .cmp(&left.touches)
            .then_with(|| left.path.cmp(&right.path))
    });
    hotspots.truncate(max_results);

    let mut ownership = build_ownership_hints(ownership_counts);
    ownership.sort_by(|left, right| {
        right
            .primary_author_touches
            .cmp(&left.primary_author_touches)
            .then_with(|| left.path.cmp(&right.path))
    });
    ownership.truncate(max_results);

    let mut co_changes = build_co_changes(co_change_counts);
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
        // change_sets are newest-first; the first match is the most recent touch.
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

    Ok(GitPathHistoryInsights {
        path: target_path.to_string(),
        status,
        commits,
        hotspot,
        ownership,
        co_change_partners,
    })
}

fn record_change_set(
    commit: &GitCommitSummary,
    changed_paths: &[String],
    file_touches: &mut HashMap<String, (usize, String, String)>,
    ownership_counts: &mut HashMap<String, BTreeMap<String, usize>>,
) {
    for path in changed_paths {
        // commits are newest-first; or_insert_with fires on first (= most recent) touch,
        // so the stored revision/summary always reflect the most recent commit for that path.
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
}

fn record_co_changes(
    changed_paths: &[String],
    co_change_counts: &mut BTreeMap<(String, String), usize>,
) {
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

fn build_hotspots(file_touches: HashMap<String, (usize, String, String)>) -> Vec<GitFileHotspot> {
    file_touches
        .into_iter()
        .map(
            |(path, (touches, last_revision, last_summary))| GitFileHotspot {
                path,
                touches,
                last_revision,
                last_summary,
            },
        )
        .collect()
}

fn build_ownership_hints(
    ownership_counts: HashMap<String, BTreeMap<String, usize>>,
) -> Vec<GitOwnershipHint> {
    ownership_counts
        .into_iter()
        .map(|(path, authors)| {
            let total_touches = authors.values().sum();
            let (primary_author, primary_author_touches) = primary_author(authors)
                .expect("path only enters ownership_counts via record_change_set, which always inserts an author");
            GitOwnershipHint {
                path,
                primary_author,
                primary_author_touches,
                total_touches,
            }
        })
        .collect()
}

/// Return the author with the highest touch count, tie-breaking by name descending.
fn primary_author(authors: BTreeMap<String, usize>) -> Option<(String, usize)> {
    authors
        .into_iter()
        .max_by(|left, right| left.1.cmp(&right.1).then_with(|| right.0.cmp(&left.0)))
}

fn build_co_changes(co_change_counts: BTreeMap<(String, String), usize>) -> Vec<GitCoChange> {
    co_change_counts
        .into_iter()
        .map(|((left_path, right_path), co_change_count)| GitCoChange {
            left_path,
            right_path,
            co_change_count,
        })
        .collect()
}
