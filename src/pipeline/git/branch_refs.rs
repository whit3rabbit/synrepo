//! Read-only Git branch-ref snapshots used as discovery roots.

use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
};

use crate::config::Config;

const CACHE_DIR: &str = "branch-cache";
const COMPLETE_SUFFIX: &str = ".complete";

/// Current commit for a configured branch ref.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BranchRefHead {
    /// Full canonical ref name, e.g. `refs/heads/main`.
    pub ref_name: String,
    /// Stable discovery root id derived from the ref name.
    pub root_id: String,
    /// Commit object id the ref currently resolves to.
    pub commit: String,
}

/// Summary of branch snapshot preparation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BranchSnapshotReport {
    /// Configured refs with a prepared local cache.
    pub prepared: usize,
    /// Configured refs that were missing or did not peel to a commit.
    pub unavailable: usize,
}

/// Derive the stable discovery root id for an exact branch ref.
pub fn branch_root_id(ref_name: &str) -> String {
    let hash = blake3::hash(ref_name.as_bytes());
    format!("branch_{}", &hex::encode(hash.as_bytes())[..32])
}

/// Resolve configured local refs without fetching.
pub fn branch_ref_heads(repo_root: &Path, config: &Config) -> Vec<BranchRefHead> {
    let Ok(repo) = gix::discover(repo_root) else {
        return Vec::new();
    };
    config
        .branch_roots
        .refs
        .iter()
        .filter_map(|ref_name| resolve_ref_head(&repo, ref_name))
        .collect()
}

/// Prepare read-only cache directories for all configured local refs.
pub fn prepare_branch_snapshots(
    repo_root: &Path,
    config: &Config,
    synrepo_dir: &Path,
) -> crate::Result<BranchSnapshotReport> {
    if config.branch_roots.refs.is_empty() {
        return Ok(BranchSnapshotReport::default());
    }
    config.branch_roots.validate()?;
    let repo = gix::discover(repo_root).map_err(|err| crate::Error::Git(err.to_string()))?;
    let mut report = BranchSnapshotReport::default();
    for ref_name in &config.branch_roots.refs {
        let Some(head) = resolve_ref_head(&repo, ref_name) else {
            report.unavailable += 1;
            continue;
        };
        prepare_one_snapshot(&repo, config, synrepo_dir, &head)?;
        report.prepared += 1;
    }
    Ok(report)
}

/// Return prepared branch roots whose cache matches the current local ref.
pub fn discover_prepared_branch_roots(
    repo_root: &Path,
    config: &Config,
    synrepo_dir: &Path,
) -> Vec<super::GitDiscoveryRoot> {
    branch_ref_heads(repo_root, config)
        .into_iter()
        .filter_map(|head| {
            let cache_root = snapshot_path(synrepo_dir, &head.root_id, &head.commit);
            if !cache_root.is_dir() || !complete_path(synrepo_dir, &head).exists() {
                return None;
            }
            Some(super::GitDiscoveryRoot {
                absolute_path: cache_root,
                discriminant: Some(head.root_id),
                kind: super::GitDiscoveryRootKind::BranchRef,
                ref_name: Some(head.ref_name),
                commit: Some(head.commit),
            })
        })
        .collect()
}

fn resolve_ref_head(repo: &gix::Repository, ref_name: &str) -> Option<BranchRefHead> {
    let mut reference = repo.find_reference(ref_name).ok()?;
    let commit = reference.peel_to_commit().ok()?;
    Some(BranchRefHead {
        ref_name: reference.name().to_string(),
        root_id: branch_root_id(ref_name),
        commit: commit.id().to_string(),
    })
}

fn prepare_one_snapshot(
    repo: &gix::Repository,
    config: &Config,
    synrepo_dir: &Path,
    head: &BranchRefHead,
) -> crate::Result<()> {
    let root_cache = synrepo_dir.join(CACHE_DIR).join(&head.root_id);
    let target = snapshot_path(synrepo_dir, &head.root_id, &head.commit);
    let marker = complete_path(synrepo_dir, head);
    if target.is_dir() && marker.exists() {
        cleanup_old_snapshots(&root_cache, &head.commit)?;
        return Ok(());
    }

    fs::create_dir_all(&root_cache)?;
    if target.exists() {
        fs::remove_dir_all(&target)?;
    }
    let temp = root_cache.join(format!(
        ".tmp.{}.{}",
        std::process::id(),
        &head.commit[..12]
    ));
    if temp.exists() {
        fs::remove_dir_all(&temp)?;
    }
    fs::create_dir_all(&temp)?;

    let commit_id = gix::ObjectId::from_hex(head.commit.as_bytes())
        .map_err(|err| crate::Error::Git(err.to_string()))?;
    let tree = repo
        .find_commit(commit_id)
        .map_err(|err| crate::Error::Git(err.to_string()))?
        .tree()
        .map_err(|err| crate::Error::Git(err.to_string()))?;
    let redaction =
        crate::substrate::discover::build_redaction_matcher(&temp, &config.redact_globs)?;

    for entry in tree
        .traverse()
        .breadthfirst
        .files()
        .map_err(|err| crate::Error::Git(err.to_string()))?
    {
        if !entry.mode.is_blob() {
            continue;
        }
        let path = String::from_utf8_lossy(entry.filepath.as_ref()).into_owned();
        let Some(relative) = safe_tree_path(&path) else {
            continue;
        };
        if !crate::substrate::discover::is_within_configured_roots(&relative, &config.roots) {
            continue;
        }
        if redaction
            .matched_path_or_any_parents(&relative, false)
            .is_ignore()
        {
            continue;
        }
        let blob = repo
            .find_blob(entry.oid)
            .map_err(|err| crate::Error::Git(err.to_string()))?;
        if blob.data.len() as u64 > config.max_file_size_bytes {
            continue;
        }
        write_snapshot_file(&temp, &relative, &blob.data)?;
    }

    fs::rename(&temp, &target)?;
    fs::write(&marker, b"complete\n")?;
    cleanup_old_snapshots(&root_cache, &head.commit)?;
    Ok(())
}

fn safe_tree_path(path: &str) -> Option<PathBuf> {
    let relative = Path::new(path);
    if relative.is_absolute() {
        return None;
    }
    for component in relative.components() {
        match component {
            Component::Normal(_) => {}
            Component::CurDir
            | Component::ParentDir
            | Component::RootDir
            | Component::Prefix(_) => {
                return None;
            }
        }
    }
    Some(relative.to_path_buf())
}

fn write_snapshot_file(root: &Path, relative: &Path, contents: &[u8]) -> crate::Result<()> {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::File::create(path)?;
    file.write_all(contents)?;
    Ok(())
}

fn snapshot_path(synrepo_dir: &Path, root_id: &str, commit: &str) -> PathBuf {
    synrepo_dir.join(CACHE_DIR).join(root_id).join(commit)
}

fn complete_path(synrepo_dir: &Path, head: &BranchRefHead) -> PathBuf {
    synrepo_dir
        .join(CACHE_DIR)
        .join(&head.root_id)
        .join(format!("{}{}", head.commit, COMPLETE_SUFFIX))
}

fn cleanup_old_snapshots(root_cache: &Path, current_commit: &str) -> crate::Result<()> {
    let Ok(entries) = fs::read_dir(root_cache) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(".tmp.") {
            let _ = fs::remove_dir_all(path);
            continue;
        }
        if name == current_commit || name == format!("{current_commit}{COMPLETE_SUFFIX}") {
            continue;
        }
        if path.is_dir() {
            let _ = fs::remove_dir_all(path);
        } else if name.ends_with(COMPLETE_SUFFIX) {
            let _ = fs::remove_file(path);
        }
    }
    Ok(())
}

pub(crate) fn head_map(repo_root: &Path, config: &Config) -> BTreeMap<String, String> {
    branch_ref_heads(repo_root, config)
        .into_iter()
        .map(|head| (head.ref_name, head.commit))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_root_id_is_ref_stable() {
        let first = branch_root_id("refs/heads/main");
        let second = branch_root_id("refs/heads/main");
        assert_eq!(first, second);
        assert_ne!(first, branch_root_id("refs/heads/feature"));
        assert!(first.starts_with("branch_"));
    }
}
