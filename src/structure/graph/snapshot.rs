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

use parking_lot::RwLock;

use super::Graph;

static GRAPH_SNAPSHOTS: LazyLock<RwLock<HashMap<PathBuf, Arc<Graph>>>> =
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
    GRAPH_SNAPSHOTS
        .read()
        .get(&canonical_key(repo_root))
        .cloned()
}

/// Publish a fully-built graph snapshot for `repo_root` atomically.
///
/// Subsequent `current(repo_root)` calls observe this snapshot. Other repos'
/// snapshots are unaffected.
pub fn publish(repo_root: &Path, new: Graph) {
    GRAPH_SNAPSHOTS
        .write()
        .insert(canonical_key(repo_root), Arc::new(new));
}

/// Drop the snapshot for `repo_root`, if any. Used by teardown paths and
/// tests that want a fresh-start guarantee.
pub fn forget(repo_root: &Path) {
    GRAPH_SNAPSHOTS.write().remove(&canonical_key(repo_root));
}

#[cfg(test)]
mod tests {
    use super::{current, forget, publish};
    use crate::structure::graph::Graph;
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
}
