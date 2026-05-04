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

    pub(super) fn retain_unsuppressed(&mut self, paths: &mut Vec<PathBuf>) {
        self.prune();
        paths.retain(|path| !self.is_suppressed(path));
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
    path == suppressed
        || path.starts_with(suppressed)
        || suppressed.starts_with(path)
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
    fn suppresses_parent_paths_reported_for_atomic_rename() {
        let mut suppressed = SuppressedPaths::default();
        suppressed.suppress(
            vec![PathBuf::from("/repo/src/a.rs")],
            Duration::from_secs(1),
        );
        let mut paths = vec![PathBuf::from("/repo/src"), PathBuf::from("/repo/other.rs")];

        suppressed.retain_unsuppressed(&mut paths);

        assert_eq!(paths, vec![PathBuf::from("/repo/other.rs")]);
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
}
