//! Process-global access to in-memory graph snapshots, keyed by repo root.
//!
//! Why per-repo: in production, one process serves one repo (a `synrepo mcp`
//! invocation, or the watch service for a single repo). A flat singleton
//! works there. But in `cargo test`, many bootstraps run concurrently across
//! distinct tempdir repos in the same test binary process. A flat singleton
//! lets the *latest* publisher's graph leak into every reader, regardless of
//! which repo the reader is asking about — `resolve_target("helper")` then
//! returns "not found" because the singleton holds another test's graph.
//!
//! Keying by canonical `repo_root` keeps each repo's snapshot independent
//! and matches the real isolation boundary.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use parking_lot::RwLock;

use super::Graph;

/// Maximum number of repository snapshots retained concurrently in the process-global cache.
pub const MAX_SNAPSHOT_REPOS: usize = 32;

/// Maximum aggregate approximate memory (1 GiB) across all cached graph snapshots before LRU eviction.
pub const MAX_AGGREGATE_BYTES: u64 = 1024 * 1024 * 1024; // 1 GiB

/// Idle duration after which a snapshot with no external readers is eligible for eviction.
pub const SNAPSHOT_IDLE_TTL: Duration = Duration::from_secs(30 * 60);

#[derive(Clone, Debug)]
pub(crate) struct SnapshotEntry {
    pub(crate) graph: Arc<Graph>,
    pub(crate) approx_bytes: u64,
    pub(crate) last_used: Instant,
}

static GRAPH_SNAPSHOTS: LazyLock<RwLock<HashMap<PathBuf, SnapshotEntry>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Canonicalise the repo root so callers using a relative or symlinked path
/// hash to the same key the publisher used. Falls back to the raw path when
/// canonicalisation fails (e.g. the directory was already deleted).
fn canonical_key(repo_root: &Path) -> PathBuf {
    std::fs::canonicalize(repo_root).unwrap_or_else(|_| repo_root.to_path_buf())
}

/// Load the most recently published snapshot for `repo_root`, if any.
///
/// Returns `None` when no publisher has touched this repo yet (typical in
/// tests that haven't called `bootstrap`) or when snapshot publishing is
/// disabled in config (`max_graph_snapshot_bytes = 0`). Callers should fall
/// back to the per-repo SQLite store in that case.
pub fn current(repo_root: &Path) -> Option<Arc<Graph>> {
    let key = canonical_key(repo_root);
    let mut snapshots = GRAPH_SNAPSHOTS.write();
    if let Some(entry) = snapshots.get_mut(&key) {
        entry.last_used = Instant::now();
        Some(Arc::clone(&entry.graph))
    } else {
        None
    }
}

/// Publish a fully-built graph snapshot for `repo_root` atomically.
///
/// Subsequent `current(repo_root)` calls observe this snapshot. Other repos'
/// snapshots are unaffected.
pub fn publish(repo_root: &Path, new: Graph) {
    let approx_bytes = new.approx_bytes() as u64;
    let entry = SnapshotEntry {
        graph: Arc::new(new),
        approx_bytes,
        last_used: Instant::now(),
    };
    let mut snapshots = GRAPH_SNAPSHOTS.write();
    snapshots.insert(canonical_key(repo_root), entry);
    evict_locked(&mut snapshots, Instant::now());
}

/// Drop the snapshot for `repo_root`, if any. Used by teardown paths and
/// tests that want a fresh-start guarantee.
pub fn forget(repo_root: &Path) {
    GRAPH_SNAPSHOTS.write().remove(&canonical_key(repo_root));
}

pub(crate) fn evict_locked(map: &mut HashMap<PathBuf, SnapshotEntry>, now: Instant) {
    // 1. 30-min TTL sweep for idle entries
    map.retain(|_, entry| {
        Arc::strong_count(&entry.graph) > 1
            || now.duration_since(entry.last_used) < SNAPSHOT_IDLE_TTL
    });

    // 2. 32-repo cap: evict idle entries oldest-last_used first
    if map.len() > MAX_SNAPSHOT_REPOS {
        let mut idle_keys: Vec<(PathBuf, Instant)> = map
            .iter()
            .filter(|(_, entry)| Arc::strong_count(&entry.graph) == 1)
            .map(|(path, entry)| (path.clone(), entry.last_used))
            .collect();
        idle_keys.sort_by_key(|(_, last_used)| *last_used);

        let excess = map.len().saturating_sub(MAX_SNAPSHOT_REPOS);
        for (path, _) in idle_keys.into_iter().take(excess) {
            map.remove(&path);
        }
    }

    // 3. 1 GiB aggregate budget: evict idle entries oldest-last_used first
    let mut total_bytes: u64 = map.values().map(|e| e.approx_bytes).sum();
    if total_bytes > MAX_AGGREGATE_BYTES {
        let mut idle_entries: Vec<(PathBuf, Instant, u64)> = map
            .iter()
            .filter(|(_, entry)| Arc::strong_count(&entry.graph) == 1)
            .map(|(path, entry)| (path.clone(), entry.last_used, entry.approx_bytes))
            .collect();
        idle_entries.sort_by_key(|(_, last_used, _)| *last_used);

        for (path, _, bytes) in idle_entries {
            if total_bytes <= MAX_AGGREGATE_BYTES {
                break;
            }
            map.remove(&path);
            total_bytes = total_bytes.saturating_sub(bytes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        current, evict_locked, forget, publish, SnapshotEntry, MAX_AGGREGATE_BYTES,
        MAX_SNAPSHOT_REPOS, SNAPSHOT_IDLE_TTL,
    };
    use crate::structure::graph::Graph;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use tempfile::tempdir;

    #[test]
    fn publish_replaces_the_current_graph_for_the_same_repo() {
        let repo = tempdir().unwrap();
        let mut first = Graph::empty();
        first.snapshot_epoch = 1;
        publish(repo.path(), first);
        assert_eq!(current(repo.path()).unwrap().snapshot_epoch, 1);

        let mut second = Graph::empty();
        second.snapshot_epoch = 2;
        publish(repo.path(), second);
        assert_eq!(current(repo.path()).unwrap().snapshot_epoch, 2);
        forget(repo.path());
    }

    #[test]
    fn snapshots_are_independent_across_repos() {
        let repo_a = tempdir().unwrap();
        let repo_b = tempdir().unwrap();
        let mut a = Graph::empty();
        a.snapshot_epoch = 7;
        let mut b = Graph::empty();
        b.snapshot_epoch = 42;
        publish(repo_a.path(), a);
        publish(repo_b.path(), b);
        assert_eq!(current(repo_a.path()).unwrap().snapshot_epoch, 7);
        assert_eq!(current(repo_b.path()).unwrap().snapshot_epoch, 42);
        forget(repo_a.path());
        forget(repo_b.path());
    }

    #[test]
    fn evict_locked_ttl_removes_expired_idle_entries_only() {
        let mut map = HashMap::new();
        let now = Instant::now();
        let old_time = now - SNAPSHOT_IDLE_TTL - Duration::from_secs(10);
        let recent_time = now - Duration::from_secs(10);

        let path1 = PathBuf::from("/repo1");
        let path2 = PathBuf::from("/repo2");
        let path3 = PathBuf::from("/repo3");

        // path1: expired and idle -> should evict
        map.insert(
            path1.clone(),
            SnapshotEntry {
                graph: Arc::new(Graph::empty()),
                approx_bytes: 100,
                last_used: old_time,
            },
        );

        // path2: expired but active reference (strong_count > 1) -> should retain
        let active_graph = Arc::new(Graph::empty());
        let _held_ref = Arc::clone(&active_graph);
        map.insert(
            path2.clone(),
            SnapshotEntry {
                graph: active_graph,
                approx_bytes: 100,
                last_used: old_time,
            },
        );

        // path3: recent and idle -> should retain
        map.insert(
            path3.clone(),
            SnapshotEntry {
                graph: Arc::new(Graph::empty()),
                approx_bytes: 100,
                last_used: recent_time,
            },
        );

        evict_locked(&mut map, now);
        assert!(!map.contains_key(&path1));
        assert!(map.contains_key(&path2));
        assert!(map.contains_key(&path3));
    }

    #[test]
    fn evict_locked_respects_repo_cap_and_budget() {
        let mut map = HashMap::new();
        let now = Instant::now();

        // 1. Repo cap eviction
        for i in 0..(MAX_SNAPSHOT_REPOS + 5) {
            let path = PathBuf::from(format!("/repo_{i}"));
            map.insert(
                path,
                SnapshotEntry {
                    graph: Arc::new(Graph::empty()),
                    approx_bytes: 100,
                    last_used: now + Duration::from_millis(i as u64),
                },
            );
        }
        assert_eq!(map.len(), MAX_SNAPSHOT_REPOS + 5);
        evict_locked(&mut map, now);
        assert_eq!(map.len(), MAX_SNAPSHOT_REPOS);
        // oldest 5 (/repo_0 through /repo_4) should have been evicted
        for i in 0..5 {
            assert!(!map.contains_key(&PathBuf::from(format!("/repo_{i}"))));
        }

        // 2. Budget eviction
        let mut budget_map = HashMap::new();
        let big_bytes = MAX_AGGREGATE_BYTES / 2 + 1000;
        let path_a = PathBuf::from("/budget_a");
        let path_b = PathBuf::from("/budget_b");
        let path_c = PathBuf::from("/budget_c");

        budget_map.insert(
            path_a.clone(),
            SnapshotEntry {
                graph: Arc::new(Graph::empty()),
                approx_bytes: big_bytes,
                last_used: now,
            },
        );
        budget_map.insert(
            path_b.clone(),
            SnapshotEntry {
                graph: Arc::new(Graph::empty()),
                approx_bytes: big_bytes,
                last_used: now + Duration::from_secs(1),
            },
        );
        // path_c is active reference
        let active_graph = Arc::new(Graph::empty());
        let _held_ref = Arc::clone(&active_graph);
        budget_map.insert(
            path_c.clone(),
            SnapshotEntry {
                graph: active_graph,
                approx_bytes: 100,
                last_used: now - Duration::from_secs(10),
            },
        );

        evict_locked(&mut budget_map, now);
        // total idle bytes > MAX_AGGREGATE_BYTES, oldest idle (path_a) evicted
        assert!(!budget_map.contains_key(&path_a));
        assert!(budget_map.contains_key(&path_b));
        assert!(budget_map.contains_key(&path_c));
    }
}
