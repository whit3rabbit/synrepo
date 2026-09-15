//! Cheap, change-driven invalidation for cached dashboard status.

use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use crate::config::Config;

use super::snapshot_refresh::SnapshotRefreshMode;

const STATUS_SOURCE_POLL_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone, Debug, Eq, PartialEq)]
struct FileStamp {
    len: u64,
    modified: Option<SystemTime>,
}

impl From<Metadata> for FileStamp {
    fn from(metadata: Metadata) -> Self {
        Self {
            len: metadata.len(),
            modified: metadata.modified().ok(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct StatusSourceFingerprint {
    full: Vec<Option<FileStamp>>,
    cached: Vec<Option<FileStamp>>,
}

impl StatusSourceFingerprint {
    fn capture(repo_root: &Path) -> Self {
        let synrepo_dir = Config::synrepo_dir(repo_root);
        let state_dir = synrepo_dir.join("state");
        let graph_dir = synrepo_dir.join("graph");
        let overlay_dir = synrepo_dir.join("overlay");

        Self {
            // These sources can change graph counts, commentary coverage, or
            // config-derived readiness. Their next node-count snapshot must
            // be exact.
            full: stamps(&[
                synrepo_dir.join("config.toml"),
                state_dir.join("reconcile-state.json"),
                graph_dir.join("nodes.db"),
                graph_dir.join("nodes.db-wal"),
                overlay_dir.join("overlay.db"),
                overlay_dir.join("overlay.db-wal"),
            ]),
            // These sources affect operational labels only. Reuse exact node
            // counts instead of recounting the graph when they change.
            cached: stamps(&[
                state_dir.join("watch-daemon.json"),
                state_dir.join("writer.lock"),
                state_dir.join("repair-log.jsonl"),
                state_dir.join("repair-log-degraded.flag"),
                state_dir.join("compact-state.json"),
                state_dir.join("explain-totals.json"),
                state_dir.join("context-metrics.json"),
            ]),
        }
    }

    fn change_from(&self, previous: &Self) -> Option<SnapshotRefreshMode> {
        if self.full != previous.full {
            Some(SnapshotRefreshMode::Full)
        } else if self.cached != previous.cached {
            Some(SnapshotRefreshMode::Cached)
        } else {
            None
        }
    }
}

fn stamps(paths: &[PathBuf]) -> Vec<Option<FileStamp>> {
    paths
        .iter()
        .map(|path| std::fs::metadata(path).ok().map(FileStamp::from))
        .collect()
}

pub(super) struct StatusChangeDetector {
    observed: StatusSourceFingerprint,
    last_poll: Instant,
}

impl StatusChangeDetector {
    pub(super) fn new(repo_root: &Path) -> Self {
        Self {
            observed: StatusSourceFingerprint::capture(repo_root),
            last_poll: Instant::now(),
        }
    }

    /// Check only metadata, and request a snapshot rebuild only when a source
    /// used by the status surface actually changed.
    pub(super) fn poll(&mut self, repo_root: &Path) -> Option<SnapshotRefreshMode> {
        if self.last_poll.elapsed() < STATUS_SOURCE_POLL_INTERVAL {
            return None;
        }
        self.last_poll = Instant::now();
        self.poll_now(repo_root)
    }

    pub(super) fn acknowledge(&mut self, repo_root: &Path) {
        self.observed = StatusSourceFingerprint::capture(repo_root);
        self.last_poll = Instant::now();
    }

    fn poll_now(&mut self, repo_root: &Path) -> Option<SnapshotRefreshMode> {
        let current = StatusSourceFingerprint::capture(repo_root);
        let change = current.change_from(&self.observed);
        self.observed = current;
        change
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unchanged_sources_do_not_request_refresh() {
        let repo = tempfile::tempdir().unwrap();
        let mut detector = StatusChangeDetector::new(repo.path());

        assert_eq!(detector.poll_now(repo.path()), None);
    }

    #[test]
    fn reconcile_state_requests_exact_refresh() {
        let repo = tempfile::tempdir().unwrap();
        let state = repo.path().join(".synrepo/state");
        std::fs::create_dir_all(&state).unwrap();
        let mut detector = StatusChangeDetector::new(repo.path());

        std::fs::write(state.join("reconcile-state.json"), b"changed").unwrap();

        assert_eq!(
            detector.poll_now(repo.path()),
            Some(SnapshotRefreshMode::Full)
        );
    }

    #[test]
    fn watch_state_requests_cached_refresh() {
        let repo = tempfile::tempdir().unwrap();
        let state = repo.path().join(".synrepo/state");
        std::fs::create_dir_all(&state).unwrap();
        let mut detector = StatusChangeDetector::new(repo.path());

        std::fs::write(state.join("watch-daemon.json"), b"changed").unwrap();

        assert_eq!(
            detector.poll_now(repo.path()),
            Some(SnapshotRefreshMode::Cached)
        );
    }
}
