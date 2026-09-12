use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Default)]
pub(super) struct SuppressedPaths {
    entries: Vec<(PathBuf, Instant)>,
}

impl SuppressedPaths {
    pub(super) fn suppress(&mut self, paths: Vec<PathBuf>, ttl: Duration) {
        let expires_at = Instant::now() + ttl;
        self.prune();
        for path in paths {
            if let Some(canonical) = canonicalize_lossy(&path) {
                self.entries.push((canonical, expires_at));
            }
            self.entries.push((path, expires_at));
        }
    }

    #[cfg(test)]
    pub(super) fn retain_unsuppressed(&mut self, paths: &mut Vec<PathBuf>) {
        self.prune();
        paths.retain(|path| !self.is_suppressed(path));
    }

    pub(super) fn filter_collected(
        &mut self,
        collected: &mut crate::pipeline::watch::filter::CollectedPaths,
    ) {
        self.prune();
        collected.paths.retain(|path| !self.is_suppressed(path));
        collected
            .directory_paths
            .retain(|dir| !self.is_suppressed(dir) && !self.is_parent_of_suppressed(dir));
    }

    fn is_parent_of_suppressed(&self, dir: &Path) -> bool {
        let canonical_dir = canonicalize_lossy(dir);
        self.entries.iter().any(|(suppressed, _)| {
            suppressed.starts_with(dir)
                || canonical_dir
                    .as_deref()
                    .is_some_and(|cd| suppressed.starts_with(cd))
        })
    }

    fn prune(&mut self) {
        let now = Instant::now();
        self.entries.retain(|(_, expires_at)| *expires_at > now);
    }

    fn is_suppressed(&self, path: &Path) -> bool {
        let canonical = canonicalize_lossy(path);
        self.entries.iter().any(|(suppressed, _)| {
            paths_overlap(path, suppressed)
                || canonical
                    .as_deref()
                    .is_some_and(|canonical_path| paths_overlap(canonical_path, suppressed))
        })
    }
}

fn paths_overlap(path: &Path, suppressed: &Path) -> bool {
    // Exact match, or `path` is a descendant of an explicitly suppressed directory,
    // or an atomic-save temporary sibling file (.target.tmp.xxx).
    // Note: `suppressed.starts_with(path)` is excluded on purpose: a parent-directory
    // notification (e.g. `src/`) must not be suppressed merely because a child
    // (`src/a.rs`) was suppressed. A parent notification means "check this directory",
    // not "ignore everything beneath it".
    path == suppressed
        || path.starts_with(suppressed)
        || is_atomic_write_temp_sibling(path, suppressed)
}

fn is_atomic_write_temp_sibling(path: &Path, suppressed: &Path) -> bool {
    if path.parent() != suppressed.parent() {
        return false;
    }
    let Some(target_name) = suppressed.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let Some(path_name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    path_name.starts_with(&format!(".{target_name}.tmp."))
}

fn canonicalize_lossy(path: &Path) -> Option<PathBuf> {
    fs::canonicalize(path).ok().or_else(|| {
        let name = path.file_name()?;
        let parent = path.parent()?;
        let canonical_parent = fs::canonicalize(parent).ok()?;
        Some(canonical_parent.join(name))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retains_only_unsuppressed_paths() {
        let mut suppressed = SuppressedPaths::default();
        suppressed.suppress(
            vec![PathBuf::from("/repo/src/a.rs")],
            Duration::from_secs(1),
        );
        let mut paths = vec![
            PathBuf::from("/repo/src/a.rs"),
            PathBuf::from("/repo/src/b.rs"),
        ];

        suppressed.retain_unsuppressed(&mut paths);

        assert_eq!(paths, vec![PathBuf::from("/repo/src/b.rs")]);
    }

    #[test]
    fn does_not_suppress_parent_directory_when_child_is_suppressed() {
        let mut suppressed = SuppressedPaths::default();
        suppressed.suppress(
            vec![PathBuf::from("/repo/src/a.rs")],
            Duration::from_secs(1),
        );
        let mut paths = vec![PathBuf::from("/repo/src"), PathBuf::from("/repo/other.rs")];

        suppressed.retain_unsuppressed(&mut paths);

        // Neither the parent directory /repo/src nor /repo/other.rs should be suppressed
        assert_eq!(
            paths,
            vec![PathBuf::from("/repo/src"), PathBuf::from("/repo/other.rs")]
        );
    }

    #[test]
    fn suppresses_atomic_write_temp_sibling() {
        let mut suppressed = SuppressedPaths::default();
        suppressed.suppress(
            vec![PathBuf::from("/repo/src/a.rs")],
            Duration::from_secs(1),
        );
        let mut paths = vec![
            PathBuf::from("/repo/src/.a.rs.tmp.123.0"),
            PathBuf::from("/repo/src/.b.rs.tmp.123.0"),
        ];

        suppressed.retain_unsuppressed(&mut paths);

        assert_eq!(paths, vec![PathBuf::from("/repo/src/.b.rs.tmp.123.0")]);
    }

    #[test]
    fn does_not_suppress_unrelated_sibling_file() {
        // Regression: suppress_watch_events previously added the parent dir to
        // the suppression set, which caused paths_overlap to match src/b.rs
        // when only src/a.rs was intended to be suppressed. With only the exact
        // file path in the suppression set, src/b.rs must pass through.
        let mut suppressed = SuppressedPaths::default();
        suppressed.suppress(
            vec![PathBuf::from("/repo/src/a.rs")],
            Duration::from_secs(1),
        );
        let mut paths = vec![
            PathBuf::from("/repo/src/a.rs"),
            PathBuf::from("/repo/src/b.rs"),
        ];

        suppressed.retain_unsuppressed(&mut paths);

        // src/a.rs is suppressed; src/b.rs must NOT be.
        assert_eq!(paths, vec![PathBuf::from("/repo/src/b.rs")]);
    }
}
