//! Git-backed discovery root enumeration.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

/// A Git repository root that should participate in filesystem discovery.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitDiscoveryRoot {
    /// Absolute checkout/submodule path.
    pub absolute_path: PathBuf,
    /// Optional stable discriminator for virtual roots.
    pub discriminant: Option<String>,
    /// Source category.
    pub kind: GitDiscoveryRootKind,
    /// Full branch ref name for branch snapshot roots.
    pub ref_name: Option<String>,
    /// Commit object id for branch snapshot roots.
    pub commit: Option<String>,
}

/// Source category for a Git-backed discovery root.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GitDiscoveryRootKind {
    /// A linked git worktree.
    Worktree,
    /// An initialized git submodule.
    Submodule,
    /// A read-only snapshot of a local Git branch ref.
    BranchRef,
}

/// Enumerate non-primary roots associated with a repository.
pub fn discover_related_roots(
    repo_root: &Path,
    include_worktrees: bool,
    include_submodules: bool,
) -> Vec<GitDiscoveryRoot> {
    let mut roots = Vec::new();
    let mut seen = BTreeSet::new();
    let Ok(repo) = gix::discover(repo_root) else {
        return roots;
    };

    if include_worktrees {
        if let Ok(worktrees) = repo.worktrees() {
            for proxy in worktrees {
                let Ok(base) = proxy.base() else {
                    continue;
                };
                push_root(&mut roots, &mut seen, base, GitDiscoveryRootKind::Worktree);
            }
        }
    }

    if include_submodules {
        collect_submodule_roots(&repo, &mut roots, &mut seen, 3);
    }

    roots
}

fn collect_submodule_roots(
    repo: &gix::Repository,
    roots: &mut Vec<GitDiscoveryRoot>,
    seen: &mut BTreeSet<PathBuf>,
    depth_remaining: usize,
) {
    if depth_remaining == 0 {
        return;
    }
    let Ok(Some(submodules)) = repo.submodules() else {
        return;
    };
    for submodule in submodules {
        let Ok(work_dir) = submodule.work_dir() else {
            continue;
        };
        if !work_dir.is_dir() {
            continue;
        }
        push_root(roots, seen, work_dir, GitDiscoveryRootKind::Submodule);
        if let Ok(Some(sub_repo)) = submodule.open() {
            collect_submodule_roots(&sub_repo, roots, seen, depth_remaining - 1);
        }
    }
}

fn push_root(
    roots: &mut Vec<GitDiscoveryRoot>,
    seen: &mut BTreeSet<PathBuf>,
    path: PathBuf,
    kind: GitDiscoveryRootKind,
) {
    let absolute_path = canonical_or_original(&path);
    if seen.insert(absolute_path.clone()) {
        roots.push(GitDiscoveryRoot {
            absolute_path,
            discriminant: None,
            kind,
            ref_name: None,
            commit: None,
        });
    }
}

fn canonical_or_original(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}
