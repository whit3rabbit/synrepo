use std::{
    fs,
    path::{Path, PathBuf},
};

use notify_debouncer_full::DebouncedEvent;

use crate::{config::Config, core::path_safety::safe_join_in_repo};

pub(crate) fn ignored_generated_dirs(repo_root: &Path, config: &Config) -> Vec<PathBuf> {
    safe_join_in_repo(repo_root, &config.export_dir)
        .into_iter()
        .collect()
}

pub(crate) fn filter_repo_events(
    events: Vec<DebouncedEvent>,
    synrepo_dir: &Path,
    ignored_dirs: &[PathBuf],
) -> Vec<DebouncedEvent> {
    let canonical_synrepo_dir = canonicalize_lossy(synrepo_dir);
    let canonical_ignored_dirs: Vec<PathBuf> = ignored_dirs
        .iter()
        .filter_map(|dir| canonicalize_lossy(dir))
        .collect();
    events
        .into_iter()
        .filter(|event| {
            !event.paths.iter().all(|path| {
                path_matches_runtime(path, synrepo_dir, canonical_synrepo_dir.as_deref())
                    || path_matches_ignored_dir(path, ignored_dirs, &canonical_ignored_dirs)
            })
        })
        .collect()
}

pub(crate) fn collect_repo_paths(
    events: &[DebouncedEvent],
    repo_root: &Path,
    synrepo_dir: &Path,
    ignored_dirs: &[PathBuf],
) -> Vec<PathBuf> {
    let git_dir = repo_root.join(".git");
    let mut paths = std::collections::BTreeSet::new();
    for event in events {
        for path in &event.paths {
            if !path.starts_with(repo_root) {
                continue;
            }
            if path.starts_with(synrepo_dir) || path.starts_with(&git_dir) {
                continue;
            }
            if ignored_dirs.iter().any(|dir| path.starts_with(dir)) {
                continue;
            }
            if matches!(fs::metadata(path), Ok(md) if md.is_dir()) {
                continue;
            }
            paths.insert(path.clone());
        }
    }
    paths.into_iter().collect()
}

fn path_matches_ignored_dir(
    path: &Path,
    ignored_dirs: &[PathBuf],
    canonical_ignored_dirs: &[PathBuf],
) -> bool {
    if ignored_dirs
        .iter()
        .any(|dir| path.starts_with(dir) || dir.starts_with(path))
    {
        return true;
    }

    let Some(canonical_path) = canonicalize_lossy(path) else {
        return false;
    };
    canonical_ignored_dirs
        .iter()
        .any(|dir| canonical_path.starts_with(dir) || dir.starts_with(&canonical_path))
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
