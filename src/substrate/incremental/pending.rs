use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use ignore::gitignore::Gitignore;
use syntext::Config as SyntextConfig;

use super::super::classify::{classify_candidate, FileClass};
use super::super::discover::{build_redaction_matcher, is_within_configured_roots, read_file_head};
use crate::config::Config;

pub(super) enum QueuedAction {
    Change(PathBuf),
    Delete(PathBuf),
    Ignore,
}

#[derive(Clone, Copy)]
pub(super) enum PendingAction {
    Change,
    Delete,
}

pub(super) fn collect_pending_actions(
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

pub(super) fn normalize_relative_path(repo_root: &Path, absolute_path: &Path) -> Option<PathBuf> {
    absolute_path
        .strip_prefix(repo_root)
        .ok()
        .map(|path| PathBuf::from(path.to_string_lossy().replace('\\', "/")))
}

pub(super) fn manifest_path(config: &Config, repo_root: &Path) -> PathBuf {
    syntext_config(config, repo_root)
        .index_dir
        .join("manifest.json")
}

pub(super) fn syntext_config(config: &Config, repo_root: &Path) -> SyntextConfig {
    SyntextConfig {
        index_dir: Config::synrepo_dir(repo_root).join("index"),
        repo_root: repo_root.to_path_buf(),
        max_file_size: config.max_file_size_bytes,
        ..SyntextConfig::default()
    }
}

pub(super) fn matcher_for_repo(config: &Config, repo_root: &Path) -> crate::Result<Gitignore> {
    build_redaction_matcher(repo_root, &config.redact_globs)
}
