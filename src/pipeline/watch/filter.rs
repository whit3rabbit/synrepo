use std::{
    fs,
    path::{Path, PathBuf},
};

use notify_debouncer_full::{
    notify::event::{EventKind, ModifyKind, RenameMode},
    DebouncedEvent,
};

use crate::{config::Config, core::path_safety::safe_join_in_repo};

pub(crate) fn ignored_generated_dirs(repo_roots: &[PathBuf], config: &Config) -> Vec<PathBuf> {
    repo_roots
        .iter()
        .filter_map(|root| safe_join_in_repo(root, &config.export_dir))
        .collect()
}

pub(crate) fn filter_repo_events(
    events: Vec<DebouncedEvent>,
    repo_roots: &[PathBuf],
    repo_root: &Path,
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
            if event.paths.iter().all(|path| {
                let path = repo_normalized_path(path, repo_root, synrepo_dir);
                path_matches_runtime(&path, synrepo_dir, canonical_synrepo_dir.as_deref())
                    || path_matches_ignored_dir(&path, ignored_dirs, &canonical_ignored_dirs)
            }) {
                return false;
            }

            event.paths.iter().any(|path| {
                let path = repo_normalized_path(path, repo_root, synrepo_dir);
                is_collectable_repo_path(&path, repo_roots, synrepo_dir, ignored_dirs, &event.kind)
            })
        })
        .collect()
}

pub(crate) fn collect_repo_paths(
    events: &[DebouncedEvent],
    repo_roots: &[PathBuf],
    repo_root: &Path,
    synrepo_dir: &Path,
    ignored_dirs: &[PathBuf],
) -> Vec<PathBuf> {
    let mut paths = std::collections::BTreeSet::new();
    for event in events {
        for path in &event.paths {
            let path = repo_normalized_path(path, repo_root, synrepo_dir);
            if !path_starts_with_any_root(&path, repo_roots) {
                continue;
            }
            if path.starts_with(synrepo_dir) || path_starts_with_any_git_dir(&path, repo_roots) {
                continue;
            }
            if ignored_dirs.iter().any(|dir| path.starts_with(dir)) {
                continue;
            }
            if !is_collectable_existing_or_missing_path(&path, &event.kind) {
                continue;
            }
            paths.insert(path);
        }
    }
    paths.into_iter().collect()
}

fn is_collectable_repo_path(
    path: &Path,
    repo_roots: &[PathBuf],
    synrepo_dir: &Path,
    ignored_dirs: &[PathBuf],
    kind: &EventKind,
) -> bool {
    if !path_starts_with_any_root(path, repo_roots) {
        return false;
    }
    if path.starts_with(synrepo_dir) || path_starts_with_any_git_dir(path, repo_roots) {
        return false;
    }
    if ignored_dirs.iter().any(|dir| path.starts_with(dir)) {
        return false;
    }
    is_collectable_existing_or_missing_path(path, kind)
}

fn path_starts_with_any_root(path: &Path, repo_roots: &[PathBuf]) -> bool {
    repo_roots.iter().any(|root| path.starts_with(root))
}

fn path_starts_with_any_git_dir(path: &Path, repo_roots: &[PathBuf]) -> bool {
    repo_roots
        .iter()
        .any(|root| path.starts_with(root.join(".git")))
}

fn is_collectable_existing_or_missing_path(path: &Path, kind: &EventKind) -> bool {
    match fs::metadata(path) {
        Ok(md) => !md.is_dir(),
        Err(_) => event_can_reference_missing_path(kind),
    }
}

fn event_can_reference_missing_path(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Remove(_)
            | EventKind::Modify(ModifyKind::Name(
                RenameMode::Any | RenameMode::From | RenameMode::Both | RenameMode::Other
            ))
            | EventKind::Any
            | EventKind::Other
    )
}

fn repo_normalized_path(path: &Path, repo_root: &Path, synrepo_dir: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        let repo_path = repo_root.join(path);
        let runtime_path = synrepo_dir.join(path);
        if !repo_path.exists() && runtime_path.exists() {
            runtime_path
        } else {
            repo_path
        }
    }
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
