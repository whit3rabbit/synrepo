use std::{
    fs,
    path::{Path, PathBuf},
};

use notify_debouncer_full::DebouncedEvent;

pub(crate) fn filter_repo_events(
    events: Vec<DebouncedEvent>,
    synrepo_dir: &Path,
) -> Vec<DebouncedEvent> {
    let canonical_synrepo_dir = canonicalize_lossy(synrepo_dir);
    events
        .into_iter()
        .filter(|event| {
            !event.paths.iter().all(|path| {
                path_matches_runtime(path, synrepo_dir, canonical_synrepo_dir.as_deref())
            })
        })
        .collect()
}

fn path_matches_runtime(
    path: &Path,
    synrepo_dir: &Path,
    canonical_synrepo_dir: Option<&Path>,
) -> bool {
    if path.starts_with(synrepo_dir) || synrepo_dir.starts_with(path) {
        return true;
    }

    match (canonicalize_lossy(path), canonical_synrepo_dir) {
        (Some(canonical_path), Some(canonical_synrepo_dir)) => {
            canonical_path.starts_with(canonical_synrepo_dir)
                || canonical_synrepo_dir.starts_with(&canonical_path)
        }
        _ => false,
    }
}

fn canonicalize_lossy(path: &Path) -> Option<PathBuf> {
    fs::canonicalize(path).ok().or_else(|| {
        let name = path.file_name()?;
        let parent = path.parent()?;
        let canonical_parent = fs::canonicalize(parent).ok()?;
        Some(canonical_parent.join(name))
    })
}
