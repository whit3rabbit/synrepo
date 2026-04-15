//! Shared Git repository state inspection for deterministic pipeline consumers.
//!
//! Structural provenance only needs the current source revision today, but
//! future git-intelligence work needs explicit degraded-state handling for
//! detached heads, shallow clones, and missing repository metadata. Keep that
//! boundary here so later Git-derived features do not rediscover HEAD state ad hoc.

use std::path::{Path, PathBuf};

use crate::config::Config;
use serde::{Deserialize, Serialize};

#[cfg(test)]
pub(crate) mod test_support;

#[cfg(test)]
mod tests;

/// A snapshot of repository Git state relevant to deterministic pipeline work.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitRepositorySnapshot {
    head: GitHeadState,
    is_shallow: bool,
}

impl GitRepositorySnapshot {
    /// Inspect repository state for deterministic pipeline consumers.
    pub fn inspect(repo_root: &Path) -> Self {
        let Ok(repo) = gix::discover(repo_root) else {
            return Self {
                head: GitHeadState::Unavailable,
                is_shallow: false,
            };
        };

        let is_shallow = repo.is_shallow();
        let head = match repo.head() {
            Ok(head) => match head.id() {
                Some(id) if head.is_detached() => GitHeadState::Detached {
                    revision: id.to_string(),
                },
                Some(id) => GitHeadState::Attached {
                    revision: id.to_string(),
                },
                None => GitHeadState::Unborn,
            },
            Err(_) => GitHeadState::Unavailable,
        };

        Self { head, is_shallow }
    }

    /// Return the resolved `HEAD` state.
    pub fn head(&self) -> &GitHeadState {
        &self.head
    }

    /// Return `true` if the repository is shallow.
    pub fn is_shallow(&self) -> bool {
        self.is_shallow
    }

    /// Return the revision string suitable for provenance records.
    pub fn source_revision(&self) -> &str {
        match &self.head {
            GitHeadState::Attached { revision } | GitHeadState::Detached { revision } => {
                revision.as_str()
            }
            GitHeadState::Unborn | GitHeadState::Unavailable => "unknown",
        }
    }

    /// Return `true` when Git-derived intelligence should treat this repository as degraded.
    pub fn is_degraded(&self) -> bool {
        self.is_shallow || !matches!(self.head, GitHeadState::Attached { .. })
    }

    /// Explain which degraded-history conditions apply.
    pub fn degraded_reasons(&self) -> Vec<GitDegradedReason> {
        let mut reasons = Vec::new();
        match self.head {
            GitHeadState::Attached { .. } => {}
            GitHeadState::Detached { .. } => reasons.push(GitDegradedReason::DetachedHead),
            GitHeadState::Unborn => reasons.push(GitDegradedReason::UnbornHead),
            GitHeadState::Unavailable => reasons.push(GitDegradedReason::RepositoryUnavailable),
        }
        if self.is_shallow {
            reasons.push(GitDegradedReason::ShallowHistory);
        }
        reasons
    }
}

/// A minimal deterministic commit record for git-intelligence sampling.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GitCommitSummary {
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

/// A deterministic first-parent commit sample with touched paths.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GitCommitChangeSet {
    /// Summary information for the commit itself.
    pub commit: GitCommitSummary,
    /// Repository-relative paths touched by the commit.
    pub changed_paths: Vec<String>,
}

/// Typed input for future git-intelligence consumers.
///
/// This wrapper keeps history-mining code on top of repository snapshot state
/// and config, rather than letting new call sites open `gix` directly.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitIntelligenceContext {
    repo_root: PathBuf,
    repository: GitRepositorySnapshot,
    requested_commit_depth: u32,
}

impl GitIntelligenceContext {
    /// Inspect the repository and bind the result to the current git-intelligence config.
    pub fn inspect(repo_root: &Path, config: &Config) -> Self {
        Self {
            repo_root: repo_root.to_path_buf(),
            repository: GitRepositorySnapshot::inspect(repo_root),
            requested_commit_depth: config.git_commit_depth,
        }
    }

    /// Return the repository snapshot backing this context.
    pub fn repository(&self) -> &GitRepositorySnapshot {
        &self.repository
    }

    /// Return the configured history depth budget for deterministic mining.
    pub fn requested_commit_depth(&self) -> u32 {
        self.requested_commit_depth
    }

    /// Return the revision string suitable for provenance and git-derived metadata.
    pub fn source_revision(&self) -> &str {
        self.repository.source_revision()
    }

    /// Return the current git-intelligence readiness for this repository state.
    pub fn readiness(&self) -> GitIntelligenceReadiness {
        let reasons = self.repository.degraded_reasons();
        if reasons.is_empty() {
            GitIntelligenceReadiness::Ready
        } else {
            GitIntelligenceReadiness::Degraded { reasons }
        }
    }

    /// Return recent first-parent commit summaries, newest first.
    ///
    /// The result is capped by the configured `git_commit_depth`. Repositories
    /// without a resolvable `HEAD` return an empty history sample.
    pub fn recent_first_parent_commits(
        &self,
        limit: usize,
    ) -> crate::Result<Vec<GitCommitSummary>> {
        Ok(self
            .recent_first_parent_commit_changes(limit)?
            .into_iter()
            .map(|entry| entry.commit)
            .collect())
    }

    /// Return recent first-parent commit samples with touched paths, newest first.
    pub fn recent_first_parent_commit_changes(
        &self,
        limit: usize,
    ) -> crate::Result<Vec<GitCommitChangeSet>> {
        let limit = limit.min(self.requested_commit_depth as usize);
        if limit == 0
            || matches!(
                self.repository.head(),
                GitHeadState::Unborn | GitHeadState::Unavailable
            )
        {
            return Ok(Vec::new());
        }

        let repo =
            gix::discover(&self.repo_root).map_err(|err| crate::Error::Git(err.to_string()))?;
        let mut current = match repo.head_commit() {
            Ok(commit) => commit,
            Err(_) => return Ok(Vec::new()),
        };
        let mut commits = Vec::with_capacity(limit);

        for _ in 0..limit {
            let summary = GitCommitSummary {
                revision: current.id().to_string(),
                summary: String::from_utf8_lossy(
                    current
                        .message()
                        .map_err(|err| crate::Error::Git(err.to_string()))?
                        .summary()
                        .as_ref(),
                )
                .into_owned(),
                author_name: String::from_utf8_lossy(
                    current
                        .author()
                        .map_err(|err| crate::Error::Git(err.to_string()))?
                        .name
                        .as_ref(),
                )
                .into_owned(),
                committed_at_unix: current
                    .time()
                    .map_err(|err| crate::Error::Git(err.to_string()))?
                    .seconds,
                parent_count: current.parent_ids().count(),
            };
            let changed_paths = changed_paths_for_first_parent(&repo, &current)?;
            commits.push(GitCommitChangeSet {
                commit: summary,
                changed_paths,
            });

            let Some(parent_id) = current.parent_ids().next() else {
                break;
            };
            current = match parent_id.object() {
                Ok(object) => object.into_commit(),
                Err(_) if self.repository.is_shallow() => break,
                Err(err) => return Err(crate::Error::Git(err.to_string())),
            };
        }

        Ok(commits)
    }
}

/// Open the git repository at `repo_root`. Returns `Err` if not a git repo.
pub fn open_repo(repo_root: &Path) -> crate::Result<gix::Repository> {
    gix::discover(repo_root).map_err(|e| crate::Error::Io(std::io::Error::other(e.to_string())))
}

/// Extract file content at a given commit revision.
///
/// Returns `None` if the revision or file path cannot be resolved.
pub fn file_content_at_revision(
    repo: &gix::Repository,
    revision: &str,
    file_path: &str,
) -> Option<Vec<u8>> {
    let oid = gix::ObjectId::from_hex(revision.as_bytes()).ok()?;
    let commit = repo.find_commit(oid).ok()?;
    let tree = commit.tree().ok()?;
    let entry = tree.lookup_entry_by_path(file_path).ok()??;
    let blob = repo.find_blob(entry.object_id()).ok()?;
    Some(blob.data.to_vec())
}

fn changed_paths_for_first_parent(
    repo: &gix::Repository,
    commit: &gix::Commit<'_>,
) -> crate::Result<Vec<String>> {
    use gix::object::tree::diff::Change;

    let current_tree = commit
        .tree()
        .map_err(|err| crate::Error::Git(err.to_string()))?;
    let parent_tree = match commit.parent_ids().next() {
        Some(parent_id) => parent_id
            .object()
            .map_err(|err| crate::Error::Git(err.to_string()))?
            .into_commit()
            .tree()
            .map_err(|err| crate::Error::Git(err.to_string()))?,
        None => repo.empty_tree(),
    };

    let mut paths = Vec::new();
    let mut platform = parent_tree
        .changes()
        .map_err(|err| crate::Error::Git(err.to_string()))?;
    platform.options(|opts| {
        opts.track_path();
        opts.track_rewrites(None);
    });
    platform
        .for_each_to_obtain_tree(&current_tree, |change| {
            let (mode, location) = match change {
                Change::Addition {
                    entry_mode,
                    location,
                    ..
                } => (entry_mode, location),
                Change::Deletion {
                    entry_mode,
                    location,
                    ..
                } => (entry_mode, location),
                Change::Modification {
                    entry_mode,
                    location,
                    ..
                } => (entry_mode, location),
                // Rewrite is unreachable while track_rewrites(None) disables rename detection,
                // but kept for exhaustiveness so a future enable surfaces here automatically.
                Change::Rewrite {
                    entry_mode,
                    location,
                    ..
                } => (entry_mode, location),
            };
            if mode.is_no_tree() && !location.is_empty() {
                paths.push(String::from_utf8_lossy(location.as_ref()).into_owned());
            }
            Ok::<_, crate::Error>(std::ops::ControlFlow::Continue(()))
        })
        .map_err(|err| crate::Error::Git(err.to_string()))?;
    paths.sort();
    paths.dedup();
    Ok(paths)
}

/// Readiness state for git-intelligence work.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum GitIntelligenceReadiness {
    /// Full-history mining may proceed without degraded-state qualifiers.
    Ready,
    /// Git-derived results must be explicitly marked as degraded.
    Degraded {
        /// Degraded conditions that future git-intelligence surfaces should report.
        reasons: Vec<GitDegradedReason>,
    },
}

/// The resolved state of `HEAD`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GitHeadState {
    /// `HEAD` points to a branch ref that resolves to a revision.
    Attached {
        /// Hex SHA of the commit object. Stored as `String` to keep the public
        /// type free of the `gix` dependency.
        revision: String,
    },
    /// `HEAD` points directly at a revision instead of a branch ref.
    Detached {
        /// Hex SHA of the commit object. Stored as `String` to keep the public
        /// type free of the `gix` dependency.
        revision: String,
    },
    /// The repository exists but has no commits yet.
    Unborn,
    /// The repository could not be opened or HEAD could not be resolved.
    Unavailable,
}

/// Degraded states that future git-intelligence code should surface explicitly.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitDegradedReason {
    /// The repository is shallow and lacks full history.
    ShallowHistory,
    /// `HEAD` is detached.
    DetachedHead,
    /// The repository exists but has no commits yet.
    UnbornHead,
    /// The repository metadata could not be inspected.
    RepositoryUnavailable,
}
