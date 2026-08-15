//! Filesystem discovery: walk the configured roots and classify files.
//!
//! Respects `.gitignore`, `.git/info/exclude`, and synrepo's own `.synignore`.

use crate::config::Config;
use crate::pipeline::git::{
    discover_prepared_branch_roots, discover_related_roots, GitDiscoveryRootKind,
};
use ignore::{
    gitignore::{Gitignore, GitignoreBuilder},
    WalkBuilder,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

use super::classify::{classify_candidate, FileClass, SNIFF_HEAD_BYTES};

const PRIMARY_ROOT_DISCRIMINANT: &str = "primary";

/// A file that the discovery pass decided is worth processing.
#[derive(Clone, Debug)]
pub struct DiscoveredFile {
    /// Absolute path on disk.
    pub absolute_path: PathBuf,
    /// Stable discriminator for the root that owns this file.
    pub root_discriminant: String,
    /// Kind of discovery root that owns this file.
    pub root_kind: DiscoveryRootKind,
    /// Path relative to the repo root.
    pub relative_path: String,
    /// Classification.
    pub class: FileClass,
    /// File size in bytes.
    pub size_bytes: u64,
}

/// A filesystem root included in one discovery pass.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveryRoot {
    /// Absolute checkout/submodule path.
    pub absolute_path: PathBuf,
    /// Stable hash used to isolate file identity for this root.
    pub discriminant: String,
    /// Root source.
    pub kind: DiscoveryRootKind,
    /// Full Git ref name for branch snapshot roots.
    pub ref_name: Option<String>,
    /// Commit object id for branch snapshot roots.
    pub commit: Option<String>,
    /// False for read-only virtual roots.
    pub editable: bool,
}

/// Source category for a discovery root.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiscoveryRootKind {
    /// The repository root passed to discovery.
    Primary,
    /// A linked git worktree.
    Worktree,
    /// An initialized git submodule.
    Submodule,
    /// A read-only local Git branch-ref snapshot.
    BranchRef,
}

impl DiscoveryRootKind {
    /// Stable API label for this root kind.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Worktree => "worktree",
            Self::Submodule => "submodule",
            Self::BranchRef => "branch_ref",
        }
    }
}

/// Walk the configured roots and yield classified files.
///
/// Honors `.gitignore` via the `ignore` crate, applies size cap, applies
/// redaction globs, sniffs encoding, and walks configured git worktree and
/// submodule roots.
pub fn discover(repo_root: &Path, config: &Config) -> crate::Result<Vec<DiscoveredFile>> {
    let roots = discover_roots(repo_root, config);
    let mut discovered = BTreeMap::new();

    for root in roots {
        let redaction_matcher = build_redaction_matcher(&root.absolute_path, &config.redact_globs)?;
        walk_root(&root, config, &redaction_matcher, &mut discovered)?;
    }

    Ok(discovered.into_values().collect())
}

fn walk_root(
    root: &DiscoveryRoot,
    config: &Config,
    redaction_matcher: &Gitignore,
    discovered: &mut BTreeMap<(String, String), DiscoveredFile>,
) -> crate::Result<()> {
    let mut walker = WalkBuilder::new(&root.absolute_path);
    let use_git_ignores = root.kind != DiscoveryRootKind::BranchRef;
    walker.hidden(false);
    walker.git_ignore(use_git_ignores);
    walker.git_exclude(use_git_ignores);
    walker.git_global(use_git_ignores);
    walker.require_git(false);
    walker.follow_links(true);
    walker.add_custom_ignore_filename(".synignore");
    let canonical_root =
        std::fs::canonicalize(&root.absolute_path).unwrap_or_else(|_| root.absolute_path.clone());
    // Never descend into generated runtime indexes. Always-on, independent of
    // local ignore files. This closes feedback loops and avoids reading index
    // sidecars that may be locked by writer processes.
    // Also skip symlinks that escape the repository root boundary.
    let filter_root = canonical_root.clone();
    walker.filter_entry(move |entry| {
        if entry.file_name() == ".synrepo" || entry.file_name() == ".syntext" {
            return false;
        }
        if let Ok(canonical) = entry.path().canonicalize() {
            canonical.starts_with(&filter_root)
        } else {
            false
        }
    });

    for result in walker.build() {
        let entry = match result {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        let Some(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_file() {
            continue;
        }

        let absolute_path = entry.into_path();
        if let Ok(canonical) = absolute_path.canonicalize() {
            if !canonical.starts_with(&canonical_root) {
                continue;
            }
        } else {
            continue;
        }
        let relative_path = match absolute_path.strip_prefix(&root.absolute_path) {
            Ok(path) => path.to_path_buf(),
            Err(_) => continue,
        };
        if !is_within_configured_roots(&relative_path, &config.roots) {
            continue;
        }

        let size_bytes = match absolute_path.metadata() {
            Ok(metadata) => metadata.len(),
            Err(_) => continue,
        };
        let is_redacted = redaction_matcher
            .matched_path_or_any_parents(&relative_path, false)
            .is_ignore();

        let class = if size_bytes > config.max_file_size_bytes || is_redacted {
            classify_candidate(&relative_path, size_bytes, &[], config, is_redacted)
        } else {
            let first_bytes = read_file_head(&absolute_path)?;
            classify_candidate(
                &relative_path,
                size_bytes,
                &first_bytes,
                config,
                is_redacted,
            )
        };

        if matches!(class, FileClass::Skipped(_)) {
            continue;
        }

        let relative_path = normalize_relative_path(&relative_path);
        discovered
            .entry((root.discriminant.clone(), relative_path.clone()))
            .or_insert(DiscoveredFile {
                absolute_path,
                root_discriminant: root.discriminant.clone(),
                root_kind: root.kind,
                relative_path,
                class,
                size_bytes,
            });
    }

    Ok(())
}

/// Enumerate discovery roots for the configured repository.
pub fn discover_roots(repo_root: &Path, config: &Config) -> Vec<DiscoveryRoot> {
    let primary_path = canonical_or_original(repo_root);
    let primary = DiscoveryRoot {
        discriminant: PRIMARY_ROOT_DISCRIMINANT.to_string(),
        absolute_path: primary_path,
        kind: DiscoveryRootKind::Primary,
        ref_name: None,
        commit: None,
        editable: true,
    };

    let mut roots = vec![primary.clone()];
    let mut seen = BTreeSet::from([primary.discriminant.clone()]);
    for root in discover_related_roots(
        repo_root,
        config.include_worktrees,
        config.include_submodules,
    ) {
        let kind = match root.kind {
            GitDiscoveryRootKind::Worktree => DiscoveryRootKind::Worktree,
            GitDiscoveryRootKind::Submodule => DiscoveryRootKind::Submodule,
            GitDiscoveryRootKind::BranchRef => DiscoveryRootKind::BranchRef,
        };
        push_root(
            &mut roots,
            &mut seen,
            root.absolute_path,
            root.discriminant,
            kind,
            root.ref_name,
            root.commit,
        );
    }

    for root in discover_prepared_branch_roots(repo_root, config, &Config::synrepo_dir(repo_root)) {
        push_root(
            &mut roots,
            &mut seen,
            root.absolute_path,
            root.discriminant,
            DiscoveryRootKind::BranchRef,
            root.ref_name,
            root.commit,
        );
    }

    roots
}

fn push_root(
    roots: &mut Vec<DiscoveryRoot>,
    seen: &mut BTreeSet<String>,
    path: PathBuf,
    discriminant: Option<String>,
    kind: DiscoveryRootKind,
    ref_name: Option<String>,
    commit: Option<String>,
) {
    let absolute_path = canonical_or_original(&path);
    let discriminant = discriminant.unwrap_or_else(|| derive_root_discriminant(&absolute_path));
    if seen.insert(discriminant.clone()) {
        roots.push(DiscoveryRoot {
            absolute_path,
            discriminant,
            kind,
            ref_name,
            commit,
            editable: kind != DiscoveryRootKind::BranchRef,
        });
    }
}

fn canonical_or_original(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// Derive the stable root discriminator used to separate identical files in
/// distinct physical checkouts.
pub(crate) fn derive_root_discriminant(root: &Path) -> String {
    let stable_path = root
        .canonicalize()
        .unwrap_or_else(|_| root.to_path_buf())
        .to_string_lossy()
        .into_owned();
    hex::encode(blake3::hash(stable_path.as_bytes()).as_bytes())
}

pub(crate) fn build_redaction_matcher(
    repo_root: &Path,
    globs: &[String],
) -> crate::Result<Gitignore> {
    let mut builder = GitignoreBuilder::new(repo_root);
    for glob in globs {
        builder.add_line(None, glob).map_err(|err| {
            crate::Error::Config(format!("invalid redaction glob `{glob}`: {err}"))
        })?;
    }
    builder
        .build()
        .map_err(|err| crate::Error::Config(format!("invalid redaction matcher: {err}")))
}

pub(crate) fn read_file_head(path: &Path) -> crate::Result<Vec<u8>> {
    let mut file = File::open(path)?;
    let mut buffer = vec![0_u8; SNIFF_HEAD_BYTES];
    let bytes_read = file.read(&mut buffer)?;
    buffer.truncate(bytes_read);
    Ok(buffer)
}

fn normalize_relative_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

pub(crate) fn is_within_configured_roots(path: &Path, roots: &[String]) -> bool {
    roots.iter().any(|root| {
        if root == "." || root.is_empty() {
            return true;
        }
        let root_path = Path::new(root);
        path == root_path || path.starts_with(root_path)
    })
}

#[cfg(test)]
mod tests;
