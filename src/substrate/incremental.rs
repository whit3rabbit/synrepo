//! Incremental substrate index maintenance backed by `syntext`, complementing the full `build_index()` path for watch-driven updates.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::classify::{classify_candidate, FileClass};
use super::discover::{build_redaction_matcher, is_within_configured_roots, read_file_head};
use crate::config::Config;
use ignore::gitignore::Gitignore;
use syntext::index::Index;
use syntext::Config as SyntextConfig;

/// How the repo index was updated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IndexSyncMode {
    /// Applied queued file changes through syntext's overlay and committed them.
    Incremental,
    /// Rebuilt the whole index from scratch.
    Rebuild,
}

/// Result of syncing the repo lexical index.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IndexSyncReport {
    /// Which maintenance path was used.
    pub mode: IndexSyncMode,
    /// Number of files queued as changed.
    pub changed_paths: usize,
    /// Number of files queued as deleted/evicted.
    pub deleted_paths: usize,
}
enum QueuedAction {
    Change(PathBuf),
    Delete(PathBuf),
    Ignore,
}

#[derive(Clone, Copy)]
enum PendingAction {
    Change,
    Delete,
}
/// Incrementally update the persisted syntext index from a touched-path set.
/// Falls back to a full rebuild when the incremental state is missing or unusable.
pub fn sync_index_incremental(
    config: &Config,
    repo_root: &Path,
    touched_paths: &[PathBuf],
) -> crate::Result<IndexSyncReport> {
    match sync_index_incremental_inner(config, repo_root, touched_paths) {
        Ok(report) => Ok(report),
        Err(err @ crate::Error::Other(_))
            if err.to_string().contains("locked by another process") =>
        {
            std::thread::sleep(std::time::Duration::from_millis(20));
            sync_index_incremental_inner(config, repo_root, touched_paths)
        }
        Err(err) => Err(err),
    }
}

fn sync_index_incremental_inner(
    config: &Config,
    repo_root: &Path,
    touched_paths: &[PathBuf],
) -> crate::Result<IndexSyncReport> {
    let redaction_matcher = build_redaction_matcher(repo_root, &config.redact_globs)?;
    let pending = collect_pending_actions(config, repo_root, touched_paths, &redaction_matcher)?;
    if pending.is_empty() {
        if manifest_path(config, repo_root).exists() {
            return Ok(IndexSyncReport {
                mode: IndexSyncMode::Incremental,
                changed_paths: 0,
                deleted_paths: 0,
            });
        }
        let report = super::build_index(config, repo_root)?;
        return Ok(IndexSyncReport {
            mode: IndexSyncMode::Rebuild,
            changed_paths: report.indexed_files,
            deleted_paths: 0,
        });
    }

    let changed_paths = pending
        .values()
        .filter(|action| matches!(action, PendingAction::Change))
        .count();
    let deleted_paths = pending
        .values()
        .filter(|action| matches!(action, PendingAction::Delete))
        .count();

    if !manifest_path(config, repo_root).exists() {
        let report = super::build_index(config, repo_root)?;
        return Ok(IndexSyncReport {
            mode: IndexSyncMode::Rebuild,
            changed_paths: report.indexed_files,
            deleted_paths: 0,
        });
    }

    let syntext_config = syntext_config(config, repo_root);
    let index = match Index::open(syntext_config.clone()) {
        Ok(index) => index,
        Err(err) if should_rebuild(&err) => {
            let report = super::build_index(config, repo_root)?;
            return Ok(IndexSyncReport {
                mode: IndexSyncMode::Rebuild,
                changed_paths: report.indexed_files,
                deleted_paths: 0,
            });
        }
        Err(err) => return Err(map_index_error(err)),
    };

    for (path, action) in &pending {
        let absolute_path = repo_root.join(path);
        let result = match action {
            PendingAction::Change => index.notify_change(&absolute_path),
            PendingAction::Delete => index.notify_delete(&absolute_path),
        };
        match result {
            Ok(()) => {}
            Err(err)
                if should_rebuild(&err)
                    || matches!(err, syntext::IndexError::OverlayFull { .. }) =>
            {
                let report = super::build_index(config, repo_root)?;
                return Ok(IndexSyncReport {
                    mode: IndexSyncMode::Rebuild,
                    changed_paths: report.indexed_files,
                    deleted_paths: 0,
                });
            }
            Err(err) => return Err(map_index_error(err)),
        }
    }

    match index.commit_batch() {
        Ok(()) => {
            if let Err(err) = index.maybe_compact() {
                tracing::warn!(error = %err, "substrate incremental compaction skipped");
            }
            Ok(IndexSyncReport {
                mode: IndexSyncMode::Incremental,
                changed_paths,
                deleted_paths,
            })
        }
        Err(err)
            if should_rebuild(&err) || matches!(err, syntext::IndexError::OverlayFull { .. }) =>
        {
            let report = super::build_index(config, repo_root)?;
            Ok(IndexSyncReport {
                mode: IndexSyncMode::Rebuild,
                changed_paths: report.indexed_files,
                deleted_paths: 0,
            })
        }
        Err(err) => Err(map_index_error(err)),
    }
}

fn collect_pending_actions(
    config: &Config,
    repo_root: &Path,
    touched_paths: &[PathBuf],
    redaction_matcher: &Gitignore,
) -> crate::Result<BTreeMap<PathBuf, PendingAction>> {
    let mut pending = BTreeMap::new();
    for absolute_path in touched_paths {
        let Some(relative_path) = normalize_relative_path(repo_root, absolute_path) else {
            continue;
        };
        match queue_action(config, absolute_path, &relative_path, redaction_matcher)? {
            QueuedAction::Change(path) => {
                pending.insert(path, PendingAction::Change);
            }
            QueuedAction::Delete(path) => {
                pending.insert(path, PendingAction::Delete);
            }
            QueuedAction::Ignore => {}
        }
    }
    Ok(pending)
}

fn queue_action(
    config: &Config,
    absolute_path: &Path,
    relative_path: &Path,
    redaction_matcher: &Gitignore,
) -> crate::Result<QueuedAction> {
    if relative_path
        .components()
        .next()
        .and_then(|component| component.as_os_str().to_str())
        .is_some_and(|segment| segment == ".git" || segment == ".synrepo")
    {
        return Ok(QueuedAction::Ignore);
    }

    if !is_within_configured_roots(relative_path, &config.roots) {
        return Ok(QueuedAction::Delete(relative_path.to_path_buf()));
    }

    let metadata = match fs::metadata(absolute_path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(QueuedAction::Delete(relative_path.to_path_buf()))
        }
        Err(err) => return Err(err.into()),
    };
    if !metadata.is_file() {
        return Ok(QueuedAction::Ignore);
    }

    let is_redacted = redaction_matcher
        .matched_path_or_any_parents(relative_path, false)
        .is_ignore();
    let head = if metadata.len() > config.max_file_size_bytes || is_redacted {
        Vec::new()
    } else {
        read_file_head(absolute_path)?
    };
    let class = classify_candidate(relative_path, metadata.len(), &head, config, is_redacted);
    match class {
        FileClass::Skipped(_) => Ok(QueuedAction::Delete(relative_path.to_path_buf())),
        _ => Ok(QueuedAction::Change(relative_path.to_path_buf())),
    }
}

fn normalize_relative_path(repo_root: &Path, absolute_path: &Path) -> Option<PathBuf> {
    absolute_path
        .strip_prefix(repo_root)
        .ok()
        .map(|path| PathBuf::from(path.to_string_lossy().replace('\\', "/")))
}

fn manifest_path(config: &Config, repo_root: &Path) -> PathBuf {
    syntext_config(config, repo_root)
        .index_dir
        .join("manifest.json")
}

fn syntext_config(config: &Config, repo_root: &Path) -> SyntextConfig {
    SyntextConfig {
        index_dir: Config::synrepo_dir(repo_root).join("index"),
        repo_root: repo_root.to_path_buf(),
        max_file_size: config.max_file_size_bytes,
        ..SyntextConfig::default()
    }
}

pub(crate) fn should_rebuild(error: &syntext::IndexError) -> bool {
    matches!(
        error,
        syntext::IndexError::CorruptIndex(_)
            | syntext::IndexError::LockConflict(_)
            | syntext::IndexError::Io(_)
    )
}

fn map_index_error(error: syntext::IndexError) -> crate::Error {
    match error {
        syntext::IndexError::CorruptIndex(message) => crate::Error::Other(anyhow::anyhow!(
            "substrate index is unusable: {message}. Re-run `synrepo init` to rebuild it."
        )),
        syntext::IndexError::LockConflict(path) => crate::Error::Other(anyhow::anyhow!(
            "substrate index at {} is locked by another process",
            path.display()
        )),
        other => crate::Error::Other(anyhow::anyhow!(
            "unable to update substrate index incrementally: {other}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use tempfile::tempdir;

    fn isolated_home() -> (tempfile::TempDir, crate::config::test_home::HomeEnvGuard) {
        let home = tempfile::tempdir().unwrap();
        let guard = crate::config::test_home::HomeEnvGuard::redirect_to(home.path());
        (home, guard)
    }

    fn bootstrap_repo() -> (
        tempfile::TempDir,
        Config,
        tempfile::TempDir,
        crate::config::test_home::HomeEnvGuard,
    ) {
        let (home, home_guard) = isolated_home();
        let repo = tempdir().unwrap();
        fs::create_dir_all(repo.path().join("src")).unwrap();
        fs::write(repo.path().join("src/lib.rs"), "pub fn alpha() {}\n").unwrap();
        fs::create_dir_all(repo.path().join(".git")).unwrap();
        crate::bootstrap::bootstrap(repo.path(), None, false).unwrap();
        let config = Config::load(repo.path()).unwrap();
        (repo, config, home, home_guard)
    }

    #[test]
    fn incremental_sync_makes_new_token_searchable() {
        let (repo, config, _home, _home_guard) = bootstrap_repo();
        fs::write(
            repo.path().join("src/lib.rs"),
            "pub fn alpha() {}\npub fn beta_token() {}\n",
        )
        .unwrap();

        let report =
            sync_index_incremental(&config, repo.path(), &[repo.path().join("src/lib.rs")])
                .unwrap();
        assert!(matches!(
            report.mode,
            IndexSyncMode::Incremental | IndexSyncMode::Rebuild
        ));
        let hits = super::super::search(&config, repo.path(), "beta_token").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, PathBuf::from("src/lib.rs"));
    }

    #[test]
    fn incremental_sync_evicts_deleted_file() {
        let (repo, config, _home, _home_guard) = bootstrap_repo();
        fs::remove_file(repo.path().join("src/lib.rs")).unwrap();

        let report =
            sync_index_incremental(&config, repo.path(), &[repo.path().join("src/lib.rs")])
                .unwrap();
        assert_eq!(report.deleted_paths, 1);
        let hits = super::super::search(&config, repo.path(), "alpha").unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn incremental_sync_skips_git_and_synrepo_runtime_paths() {
        let (repo, config, _home, _home_guard) = bootstrap_repo();
        fs::write(repo.path().join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        fs::create_dir_all(repo.path().join(".synrepo/state")).unwrap();
        fs::write(repo.path().join(".synrepo/state/noise.txt"), "ignored").unwrap();

        let report = sync_index_incremental(
            &config,
            repo.path(),
            &[
                repo.path().join(".git/HEAD"),
                repo.path().join(".synrepo/state/noise.txt"),
            ],
        )
        .unwrap();
        assert_eq!(report.changed_paths, 0);
        assert_eq!(report.deleted_paths, 0);
        let hits = super::super::search(&config, repo.path(), "alpha").unwrap();
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn incremental_sync_refuses_redacted_files() {
        let (_home, _home_guard) = isolated_home();
        let repo = tempdir().unwrap();
        fs::write(repo.path().join("notes.txt"), "visible").unwrap();
        crate::bootstrap::bootstrap(repo.path(), None, false).unwrap();
        let mut config = Config::load(repo.path()).unwrap();
        config.redact_globs.push("**/*.secret".to_string());

        fs::write(repo.path().join("token.secret"), "hidden_value").unwrap();
        let report =
            sync_index_incremental(&config, repo.path(), &[repo.path().join("token.secret")])
                .unwrap();
        assert_eq!(report.deleted_paths, 1);
        let hits = super::super::search(&config, repo.path(), "hidden_value").unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn incremental_sync_falls_back_to_rebuild_when_manifest_is_missing() {
        let (repo, config, _home, _home_guard) = bootstrap_repo();
        fs::remove_file(manifest_path(&config, repo.path())).unwrap();
        fs::write(
            repo.path().join("src/lib.rs"),
            "pub fn rebuilt_token() {}\n",
        )
        .unwrap();

        let report =
            sync_index_incremental(&config, repo.path(), &[repo.path().join("src/lib.rs")])
                .unwrap();
        assert_eq!(report.mode, IndexSyncMode::Rebuild);
        let hits = super::super::search(&config, repo.path(), "rebuilt_token").unwrap();
        assert_eq!(hits.len(), 1);
    }
}
