use std::{
    fs,
    path::{Path, PathBuf},
};

use notify_debouncer_full::{
    notify::event::{EventKind, ModifyKind, RenameMode},
    DebouncedEvent,
};

use ignore::gitignore::{Gitignore, GitignoreBuilder};

use crate::{config::Config, core::path_safety::safe_join_in_repo};

pub(crate) struct WatchIgnoreSet {
    roots: Vec<RootIgnoreMatcher>,
}

struct RootIgnoreMatcher {
    root: PathBuf,
    matcher: Gitignore,
}

impl WatchIgnoreSet {
    pub(crate) fn from_roots(repo_roots: &[PathBuf]) -> Self {
        let roots = repo_roots
            .iter()
            .map(|root| RootIgnoreMatcher {
                root: root.clone(),
                matcher: build_root_ignore_matcher(root),
            })
            .collect();
        Self { roots }
    }

    fn is_ignored(&self, path: &Path) -> bool {
        self.roots.iter().any(|root| root.is_ignored(path))
    }
}

impl RootIgnoreMatcher {
    fn is_ignored(&self, path: &Path) -> bool {
        let Ok(relative_path) = path.strip_prefix(&self.root) else {
            return false;
        };
        if relative_path.as_os_str().is_empty() {
            return false;
        }
        let is_dir = fs::metadata(path)
            .map(|metadata| metadata.is_dir())
            .unwrap_or(false);
        self.matcher
            .matched_path_or_any_parents(relative_path, is_dir)
            .is_ignore()
    }
}

pub(crate) fn ignored_generated_dirs(repo_roots: &[PathBuf], config: &Config) -> Vec<PathBuf> {
    repo_roots
        .iter()
        .filter_map(|root| safe_join_in_repo(root, &config.export_dir))
        .collect()
}

struct WatchFilterContext<'a> {
    repo_roots: &'a [PathBuf],
    synrepo_dir: &'a Path,
    canonical_synrepo_dir: Option<PathBuf>,
    syntext_dir: PathBuf,
    canonical_syntext_dir: Option<PathBuf>,
    ignored_dirs: &'a [PathBuf],
    canonical_ignored_dirs: Vec<PathBuf>,
    ignore_set: &'a WatchIgnoreSet,
}

impl<'a> WatchFilterContext<'a> {
    fn new(
        repo_roots: &'a [PathBuf],
        repo_root: &Path,
        synrepo_dir: &'a Path,
        ignored_dirs: &'a [PathBuf],
        ignore_set: &'a WatchIgnoreSet,
    ) -> Self {
        let canonical_synrepo_dir = canonicalize_lossy(synrepo_dir);
        let syntext_dir = repo_root.join(".syntext");
        let canonical_syntext_dir = canonicalize_lossy(&syntext_dir);
        let canonical_ignored_dirs: Vec<PathBuf> = ignored_dirs
            .iter()
            .filter_map(|dir| canonicalize_lossy(dir))
            .collect();
        Self {
            repo_roots,
            synrepo_dir,
            canonical_synrepo_dir,
            syntext_dir,
            canonical_syntext_dir,
            ignored_dirs,
            canonical_ignored_dirs,
            ignore_set,
        }
    }

    fn matches_ignored_or_runtime(&self, path: &Path) -> bool {
        path_has_internal_component(path)
            || path_starts_with_any_synrepo_dir(path, self.repo_roots)
            || path_matches_runtime(
                path,
                self.synrepo_dir,
                self.canonical_synrepo_dir.as_deref(),
            )
            || path_matches_runtime(
                path,
                &self.syntext_dir,
                self.canonical_syntext_dir.as_deref(),
            )
            || path_starts_with_external_syntext_dir(path, self.repo_roots)
            || path_starts_with_any_git_dir(path, self.repo_roots)
            || path_matches_ignored_dir(path, self.ignored_dirs, &self.canonical_ignored_dirs)
            || self.ignore_set.is_ignored(path)
    }

    fn is_collectable(&self, path: &Path, kind: &EventKind) -> bool {
        if !path_starts_with_any_root(path, self.repo_roots) {
            return false;
        }
        if self.matches_ignored_or_runtime(path) {
            return false;
        }
        !matches!(collectable_path_kind(path, kind), CollectableKind::Skip)
    }
}

pub(crate) fn filter_repo_events(
    events: Vec<DebouncedEvent>,
    repo_roots: &[PathBuf],
    repo_root: &Path,
    synrepo_dir: &Path,
    ignored_dirs: &[PathBuf],
    ignore_set: &WatchIgnoreSet,
) -> Vec<DebouncedEvent> {
    let ctx = WatchFilterContext::new(repo_roots, repo_root, synrepo_dir, ignored_dirs, ignore_set);
    events
        .into_iter()
        .filter(|event| {
            if event.paths.iter().all(|path| {
                let path = repo_normalized_path(path, repo_root, synrepo_dir);
                ctx.matches_ignored_or_runtime(&path)
            }) {
                return false;
            }

            event.paths.iter().any(|path| {
                let path = repo_normalized_path(path, repo_root, synrepo_dir);
                ctx.is_collectable(&path, &event.kind)
            })
        })
        .collect()
}

/// Paths and metadata collected from a batch of debounced events.
#[derive(Debug)]
pub(crate) struct CollectedPaths {
    /// Individual file paths to process incrementally.
    pub paths: Vec<PathBuf>,
    /// Directory paths observed in the event batch.
    pub directory_paths: Vec<PathBuf>,
}

impl CollectedPaths {
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty() && self.directory_paths.is_empty()
    }

    pub fn has_directory_event(&self) -> bool {
        !self.directory_paths.is_empty()
    }
}

pub(crate) fn collect_repo_paths(
    events: &[DebouncedEvent],
    repo_roots: &[PathBuf],
    repo_root: &Path,
    synrepo_dir: &Path,
    ignored_dirs: &[PathBuf],
    ignore_set: &WatchIgnoreSet,
) -> CollectedPaths {
    let ctx = WatchFilterContext::new(repo_roots, repo_root, synrepo_dir, ignored_dirs, ignore_set);
    let mut paths = std::collections::BTreeSet::new();
    let mut directory_paths = std::collections::BTreeSet::new();
    for event in events {
        for path in &event.paths {
            let path = repo_normalized_path(path, repo_root, synrepo_dir);
            if !ctx.is_collectable(&path, &event.kind) {
                continue;
            }
            match collectable_path_kind(&path, &event.kind) {
                CollectableKind::File => {
                    paths.insert(path);
                }
                CollectableKind::Directory => {
                    directory_paths.insert(path);
                }
                CollectableKind::Skip => {}
            }
        }
    }
    CollectedPaths {
        paths: paths.into_iter().collect(),
        directory_paths: directory_paths.into_iter().collect(),
    }
}

fn path_starts_with_any_root(path: &Path, repo_roots: &[PathBuf]) -> bool {
    repo_roots.iter().any(|root| path.starts_with(root))
}

fn path_starts_with_any_git_dir(path: &Path, repo_roots: &[PathBuf]) -> bool {
    repo_roots
        .iter()
        .any(|root| path.starts_with(root.join(".git")))
}

fn path_starts_with_any_synrepo_dir(path: &Path, repo_roots: &[PathBuf]) -> bool {
    repo_roots
        .iter()
        .any(|root| path.starts_with(root.join(".synrepo")))
}

fn path_starts_with_external_syntext_dir(path: &Path, repo_roots: &[PathBuf]) -> bool {
    repo_roots
        .iter()
        .any(|root| path.starts_with(root.join(".syntext")))
}

fn path_has_internal_component(path: &Path) -> bool {
    path.components().any(|c| {
        let s = c.as_os_str();
        s == ".synrepo" || s == ".syntext" || s == ".git"
    })
}

fn build_root_ignore_matcher(root: &Path) -> Gitignore {
    let mut builder = GitignoreBuilder::new(root);
    for path in [
        root.join(".gitignore"),
        root.join(".git/info/exclude"),
        root.join(".synignore"),
        // `.synrepoignore` is the synrepo-native user-facing ignore layer.
        // Loaded after `.synignore` so a project can override syntext's
        // defaults with a synrepo-specific entry.
        root.join(".synrepoignore"),
    ] {
        if path.is_file() {
            let _ = builder.add(path);
        }
    }
    builder.build().unwrap_or_else(|_| Gitignore::empty())
}

/// Outcome of inspecting a single path from an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CollectableKind {
    /// A regular file that should be queued for incremental processing.
    File,
    /// An existing directory: FSEvents may coalesce child events into one
    /// directory-level notification; the caller should trigger a full scan.
    Directory,
    /// Path should be skipped (unresolvable or uninteresting kind).
    Skip,
}

fn collectable_path_kind(path: &Path, kind: &EventKind) -> CollectableKind {
    match fs::metadata(path) {
        Ok(md) => {
            if md.is_dir() {
                // On macOS, FSEvents can report a directory for moves, new
                // content, or event coalescing. Treat as full-reconcile hint.
                CollectableKind::Directory
            } else {
                CollectableKind::File
            }
        }
        Err(_) => {
            if event_can_reference_missing_path(kind) {
                CollectableKind::File
            } else {
                CollectableKind::Skip
            }
        }
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
