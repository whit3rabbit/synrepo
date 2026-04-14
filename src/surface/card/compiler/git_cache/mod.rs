//! Per-compiler cache of git-intelligence results with HEAD-change invalidation.
//!
//! Key design choices:
//! - One `GitHistoryIndex` per HEAD generation (bulk amortisation).
//! - 500 ms debounced HEAD probing (collapses bulk export traffic).
//! - FIFO-bounded path memo with `RwLock` read-fast-path.

use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::RwLock;

use crate::{
    config::Config,
    pipeline::{
        git::{GitDegradedReason, GitIntelligenceContext, GitIntelligenceReadiness},
        git_intelligence::{GitHistoryIndex, GitPathHistoryInsights},
    },
};

#[cfg(test)]
mod tests;

/// FIFO eviction; re-projecting an evicted path is cheap.
pub(super) const PATH_CACHE_CAPACITY: usize = 32_768;

/// Collapses bulk-export traffic into a handful of probes per second.
const HEAD_PROBE_DEBOUNCE: Duration = Duration::from_millis(500);

/// Three-state cache for file-scoped git intelligence, keyed by path.
pub(super) struct GitCache {
    inner: RwLock<Inner>,
}

/// `Unavailable` is a terminal latch for non-git repos.
enum Inner {
    Uninitialized,
    Unavailable,
    Ready {
        index: Arc<GitHistoryIndex>,
        head_sha: String,
        paths: BoundedPathCache,
        last_head_check: Instant,
    },
}

/// FIFO-bounded per-path memo.
struct BoundedPathCache {
    map: HashMap<String, Option<Arc<GitPathHistoryInsights>>>,
    order: VecDeque<String>,
    capacity: usize,
}

impl BoundedPathCache {
    fn new(capacity: usize) -> Self {
        Self {
            map: HashMap::new(),
            order: VecDeque::new(),
            capacity,
        }
    }

    fn get(&self, path: &str) -> Option<&Option<Arc<GitPathHistoryInsights>>> {
        self.map.get(path)
    }

    fn insert(&mut self, path: String, value: Option<Arc<GitPathHistoryInsights>>) {
        // Overwrite in place to keep FIFO position stable.
        if let Some(slot) = self.map.get_mut(&path) {
            *slot = value;
            return;
        }
        if self.map.len() >= self.capacity {
            if let Some(oldest) = self.order.pop_front() {
                self.map.remove(&oldest);
            }
        }
        self.order.push_back(path.clone());
        self.map.insert(path, value);
    }

    fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
    }
}

impl GitCache {
    pub(super) fn new() -> Self {
        Self {
            inner: RwLock::new(Inner::Uninitialized),
        }
    }

    /// Resolve git intelligence for a repo-relative path.
    pub(super) fn resolve_path(
        &self,
        repo_root: &Path,
        config: &Config,
        path: &str,
    ) -> Option<Arc<GitPathHistoryInsights>> {
        if let Some(hit) = self.read_fast_path(path) {
            return hit;
        }
        self.slow_path(repo_root, config, path)
    }

    /// Returns `Some(hit)` when debounce-fresh and path is cached, else `None`.
    fn read_fast_path(&self, path: &str) -> Option<Option<Arc<GitPathHistoryInsights>>> {
        let guard = self.inner.read();
        match &*guard {
            Inner::Ready {
                paths,
                last_head_check,
                ..
            } if last_head_check.elapsed() < HEAD_PROBE_DEBOUNCE => paths.get(path).cloned(),
            _ => None,
        }
    }

    fn slow_path(
        &self,
        repo_root: &Path,
        config: &Config,
        path: &str,
    ) -> Option<Arc<GitPathHistoryInsights>> {
        let mut guard = self.inner.write();

        if matches!(&*guard, Inner::Uninitialized) {
            *guard = initialize(repo_root, config);
        }

        match &mut *guard {
            Inner::Uninitialized => unreachable!("just initialized above"),
            Inner::Unavailable => None,
            Inner::Ready { .. } => {
                maybe_refresh_head(&mut guard, repo_root, config);
                lookup_or_project(&mut guard, path)
            }
        }
    }

    /// Test-only: reset the debounce so the next `resolve_path` call
    /// re-probes HEAD. Keeps tests deterministic without a clock abstraction.
    #[cfg(test)]
    fn force_head_probe(&self) {
        let mut guard = self.inner.write();
        if let Inner::Ready {
            last_head_check, ..
        } = &mut *guard
        {
            *last_head_check = Instant::now() - HEAD_PROBE_DEBOUNCE - Duration::from_millis(1);
        }
    }

    #[cfg(test)]
    fn index_ptr(&self) -> Option<usize> {
        match &*self.inner.read() {
            Inner::Ready { index, .. } => Some(Arc::as_ptr(index) as usize),
            _ => None,
        }
    }
}

/// Build the initial `Ready` state, latching `Unavailable` for non-git repos.
fn initialize(repo_root: &Path, config: &Config) -> Inner {
    let context = GitIntelligenceContext::inspect(repo_root, config);
    if is_repository_unavailable(&context) {
        tracing::warn!(
            repo_root = %repo_root.display(),
            "git repository unavailable; FileCard.git_intelligence and SymbolCard.last_change will be None"
        );
        return Inner::Unavailable;
    }
    let head_sha = context.source_revision().to_string();
    let index = match GitHistoryIndex::build(&context, config.git_commit_depth as usize) {
        Ok(index) => Arc::new(index),
        Err(err) => {
            tracing::debug!(%err, "GitHistoryIndex::build failed; treating repo as unavailable");
            return Inner::Unavailable;
        }
    };
    Inner::Ready {
        index,
        head_sha,
        paths: BoundedPathCache::new(PATH_CACHE_CAPACITY),
        last_head_check: Instant::now(),
    }
}

/// Re-inspect HEAD if the debounce has elapsed. Rebuilds index on SHA change.
fn maybe_refresh_head(
    guard: &mut parking_lot::RwLockWriteGuard<'_, Inner>,
    repo_root: &Path,
    config: &Config,
) {
    let needs_probe = match &**guard {
        Inner::Ready {
            last_head_check, ..
        } => last_head_check.elapsed() >= HEAD_PROBE_DEBOUNCE,
        _ => false,
    };
    if !needs_probe {
        return;
    }

    let context = GitIntelligenceContext::inspect(repo_root, config);
    if is_repository_unavailable(&context) {
        // The repo disappeared after initial success; latch `Unavailable`
        // rather than serving a stale index.
        **guard = Inner::Unavailable;
        return;
    }

    let fresh_sha = context.source_revision().to_string();
    let Inner::Ready {
        index,
        head_sha,
        paths,
        last_head_check,
    } = &mut **guard
    else {
        return;
    };

    if *head_sha == fresh_sha {
        *last_head_check = Instant::now();
        return;
    }

    match GitHistoryIndex::build(&context, config.git_commit_depth as usize) {
        Ok(new_index) => {
            *index = Arc::new(new_index);
            *head_sha = fresh_sha;
            paths.clear();
            *last_head_check = Instant::now();
            tracing::debug!("git HEAD moved; rebuilt history index");
        }
        Err(err) => {
            tracing::debug!(
                %err,
                "GitHistoryIndex::build failed during refresh; keeping previous index"
            );
            *last_head_check = Instant::now();
        }
    }
}

/// Hit the path memo or project a fresh insight from the index.
fn lookup_or_project(
    guard: &mut parking_lot::RwLockWriteGuard<'_, Inner>,
    path: &str,
) -> Option<Arc<GitPathHistoryInsights>> {
    let Inner::Ready { index, paths, .. } = &mut **guard else {
        return None;
    };

    if let Some(hit) = paths.get(path) {
        return hit.clone();
    }

    let insights = index.project_path(path, super::super::git::FILE_NODE_GIT_INSIGHT_LIMIT);
    let value = if insights.commits.is_empty()
        && insights.hotspot.is_none()
        && insights.ownership.is_none()
        && insights.co_change_partners.is_empty()
    {
        // Path never appeared in the sampled window: cache a miss so we
        // do not re-project on every subsequent lookup.
        None
    } else {
        Some(Arc::new(insights))
    };
    paths.insert(path.to_string(), value.clone());
    value
}

fn is_repository_unavailable(context: &GitIntelligenceContext) -> bool {
    match context.readiness() {
        GitIntelligenceReadiness::Ready => false,
        GitIntelligenceReadiness::Degraded { reasons } => reasons
            .iter()
            .any(|reason| matches!(reason, GitDegradedReason::RepositoryUnavailable)),
    }
}
