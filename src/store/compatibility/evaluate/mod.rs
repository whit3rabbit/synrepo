//! Runtime compatibility evaluation for `.synrepo/` stores.

use crate::config::Config;
use std::{fs, path::Path};

use super::types::{
    CompatAction, CompatibilityEntry, CompatibilityReport, ConfigFingerprints,
    RuntimeCompatibilitySnapshot, StoreClass, StoreId,
};

#[cfg(test)]
mod tests;

/// Evaluate store compatibility for the current runtime and config.
pub fn evaluate_runtime(
    synrepo_dir: &Path,
    runtime_exists: bool,
    config: &Config,
) -> crate::Result<CompatibilityReport> {
    let snapshot_path = super::snapshot::snapshot_path(synrepo_dir);
    let mut warnings = Vec::new();
    let snapshot = load_snapshot(&snapshot_path, &mut warnings)?;
    let fingerprints = ConfigFingerprints::from_config(config);
    let mut entries = Vec::new();

    for store_id in StoreId::ALL {
        let materialized = store_is_materialized(synrepo_dir, store_id)?;
        let entry = if !runtime_exists {
            CompatibilityEntry {
                store_id,
                class: store_id.class(),
                action: CompatAction::Continue,
                reason: "runtime does not exist yet".to_string(),
            }
        } else {
            evaluate_store(store_id, materialized, snapshot.as_ref())
        };
        entries.push(entry);
    }

    if let Some(snapshot) = &snapshot {
        if snapshot.config_fingerprints.index_inputs != fingerprints.index_inputs {
            set_action(
                &mut entries,
                StoreId::Index,
                CompatAction::Rebuild,
                "index-sensitive config changed (`roots`, `max_file_size_bytes`, or `redact_globs`)".to_string(),
            );
            set_action(
                &mut entries,
                StoreId::Embeddings,
                CompatAction::Invalidate,
                "index-sensitive config changed, so embeddings are stale".to_string(),
            );
            set_action(
                &mut entries,
                StoreId::LlmResponsesCache,
                CompatAction::Invalidate,
                "index-sensitive config changed, so cached responses tied to old retrieval inputs are stale".to_string(),
            );
        }

        if snapshot.config_fingerprints.graph_inputs != fingerprints.graph_inputs {
            if store_is_materialized(synrepo_dir, StoreId::Graph)? {
                set_action(
                    &mut entries,
                    StoreId::Graph,
                    CompatAction::Rebuild,
                    "graph-sensitive config changed (`concept_directories`)".to_string(),
                );
            } else {
                warnings.push(
                    "graph-sensitive config changed (`concept_directories`), but no graph store is materialized yet".to_string(),
                );
            }

            if store_is_materialized(synrepo_dir, StoreId::Overlay)? {
                set_action(
                    &mut entries,
                    StoreId::Overlay,
                    CompatAction::Invalidate,
                    "graph-sensitive config changed, so overlay material derived from the old graph assumptions is stale".to_string(),
                );
            } else {
                warnings.push(
                    "graph-sensitive config changed (`concept_directories`); later graph and overlay stores should treat this as compatibility drift".to_string(),
                );
            }
        }

        if snapshot.config_fingerprints.advisory_inputs != fingerprints.advisory_inputs
            && !snapshot.config_fingerprints.advisory_inputs.is_empty()
        {
            warnings.push(
                "advisory config changed (`cross_link_confidence_thresholds` or `cross_link_cost_limit`); existing cross-link candidates are re-classified on the next `synrepo sync`".to_string(),
            );
        }

        if snapshot.config_fingerprints.history_inputs != fingerprints.history_inputs {
            if store_is_materialized(synrepo_dir, StoreId::Graph)? {
                set_action(
                    &mut entries,
                    StoreId::Graph,
                    CompatAction::Block,
                    "history-sensitive config changed (`git_commit_depth`); reset `.synrepo/` and re-init"
                        .to_string(),
                );
            } else {
                warnings.push(
                    "history-sensitive config changed (`git_commit_depth`), but no graph store is materialized yet".to_string(),
                );
            }

            if store_is_materialized(synrepo_dir, StoreId::Overlay)? {
                set_action(
                    &mut entries,
                    StoreId::Overlay,
                    CompatAction::Invalidate,
                    "history-sensitive config changed, so overlay material derived from old Git-history inputs is stale".to_string(),
                );
            } else {
                warnings.push(
                    "history-sensitive config changed (`git_commit_depth`); later graph and overlay stores should treat this as compatibility drift".to_string(),
                );
            }
        }
    }

    Ok(CompatibilityReport {
        snapshot_path,
        entries,
        warnings,
    })
}

/// Apply non-blocking compatibility actions to the on-disk runtime.
///
/// Requires an exclusive `WriterLock` guard, enforced by the type system.
pub fn apply_runtime_actions(
    _lock: &crate::pipeline::writer::WriterLock,
    synrepo_dir: &Path,
    report: &CompatibilityReport,
) -> crate::Result<bool> {
    let mut changed = false;

    for entry in &report.entries {
        match entry.action {
            CompatAction::Continue | CompatAction::Block => {}
            CompatAction::Rebuild | CompatAction::Invalidate | CompatAction::ClearAndRecreate => {
                clear_store_contents(synrepo_dir, entry.store_id)?;
                changed = true;
            }
        }
    }

    Ok(changed)
}

fn evaluate_store(
    store_id: StoreId,
    materialized: bool,
    snapshot: Option<&RuntimeCompatibilitySnapshot>,
) -> CompatibilityEntry {
    if !materialized {
        return CompatibilityEntry {
            store_id,
            class: store_id.class(),
            action: CompatAction::Continue,
            reason: "store is not materialized".to_string(),
        };
    }

    let Some(snapshot) = snapshot else {
        return CompatibilityEntry {
            store_id,
            class: store_id.class(),
            action: default_action_without_snapshot(store_id),
            reason: "no compatibility snapshot exists for this runtime".to_string(),
        };
    };

    if snapshot.snapshot_version > super::SNAPSHOT_VERSION {
        return CompatibilityEntry {
            store_id,
            class: store_id.class(),
            action: default_action_for_newer_store(store_id),
            reason: format!(
                "snapshot version {} is newer than this runtime understands",
                snapshot.snapshot_version
            ),
        };
    }

    let stored_version = snapshot
        .store_format_versions
        .get(store_id.as_str())
        .copied()
        .unwrap_or_default();
    let expected_version = store_id.expected_format_version();

    let action = if stored_version == expected_version {
        CompatAction::Continue
    } else if stored_version == 0 {
        default_action_without_snapshot(store_id)
    } else if stored_version < expected_version {
        default_action_for_older_store(store_id)
    } else {
        default_action_for_newer_store(store_id)
    };

    let reason = if action == CompatAction::Continue {
        "store format version matches the runtime expectation".to_string()
    } else if stored_version == 0 {
        "store format version is missing from the compatibility snapshot".to_string()
    } else if stored_version < expected_version {
        format!(
            "store format version {} is older than expected version {}",
            stored_version, expected_version
        )
    } else {
        format!(
            "store format version {} is newer than expected version {}",
            stored_version, expected_version
        )
    };

    CompatibilityEntry {
        store_id,
        class: store_id.class(),
        action,
        reason,
    }
}

fn default_action_without_snapshot(store_id: StoreId) -> CompatAction {
    match store_id.class() {
        StoreClass::Canonical => CompatAction::Block,
        StoreClass::Supplemental | StoreClass::Disposable => CompatAction::Invalidate,
        StoreClass::Rebuildable => CompatAction::Rebuild,
        StoreClass::Ephemeral => CompatAction::ClearAndRecreate,
    }
}

fn default_action_for_older_store(store_id: StoreId) -> CompatAction {
    match store_id.class() {
        StoreClass::Canonical => CompatAction::Block,
        StoreClass::Supplemental | StoreClass::Disposable => CompatAction::Invalidate,
        StoreClass::Rebuildable => CompatAction::Rebuild,
        StoreClass::Ephemeral => CompatAction::ClearAndRecreate,
    }
}

fn default_action_for_newer_store(store_id: StoreId) -> CompatAction {
    match store_id.class() {
        StoreClass::Canonical => CompatAction::Block,
        StoreClass::Supplemental | StoreClass::Disposable => CompatAction::Invalidate,
        StoreClass::Rebuildable => CompatAction::Rebuild,
        StoreClass::Ephemeral => CompatAction::ClearAndRecreate,
    }
}

fn set_action(
    entries: &mut [CompatibilityEntry],
    store_id: StoreId,
    action: CompatAction,
    reason: String,
) {
    if let Some(entry) = entries.iter_mut().find(|entry| entry.store_id == store_id) {
        entry.action = action;
        entry.reason = reason;
    }
}

fn store_is_materialized(synrepo_dir: &Path, store_id: StoreId) -> crate::Result<bool> {
    let store_path = synrepo_dir.join(store_id.relative_path());
    if !store_path.exists() {
        return Ok(false);
    }
    if store_path.is_file() {
        return Ok(true);
    }

    let mut entries = fs::read_dir(store_path)?;
    Ok(entries.next().transpose()?.is_some())
}

/// Clear the contents of a specific store.
pub(crate) fn clear_store_contents(synrepo_dir: &Path, store_id: StoreId) -> crate::Result<()> {
    let store_path = synrepo_dir.join(store_id.relative_path());
    fs::create_dir_all(&store_path)?;

    let canonical_store = store_path.canonicalize().map_err(|e| {
        crate::Error::Other(anyhow::anyhow!("failed to canonicalize store path: {e}"))
    })?;

    for entry in fs::read_dir(&store_path)? {
        let entry = entry?;
        let path = entry.path();

        let meta = std::fs::symlink_metadata(&path).map_err(|e| {
            crate::Error::Other(anyhow::anyhow!(
                "failed to read metadata for {}: {e}",
                path.display()
            ))
        })?;
        if meta.file_type().is_symlink() {
            return Err(crate::Error::Other(anyhow::anyhow!(
                "refusing to follow symlink at {}",
                path.display()
            )));
        }

        let canonical_child = path.canonicalize().map_err(|e| {
            crate::Error::Other(anyhow::anyhow!(
                "failed to canonicalize child path {}: {e}",
                path.display()
            ))
        })?;
        if !canonical_child.starts_with(&canonical_store) {
            return Err(crate::Error::Other(anyhow::anyhow!(
                "path escape detected: {} is not under {}",
                path.display(),
                store_path.display()
            )));
        }

        if path.is_dir() {
            fs::remove_dir_all(path)?;
        } else {
            fs::remove_file(path)?;
        }
    }

    Ok(())
}

fn load_snapshot(
    snapshot_path: &std::path::Path,
    warnings: &mut Vec<String>,
) -> crate::Result<Option<RuntimeCompatibilitySnapshot>> {
    if !snapshot_path.exists() {
        return Ok(None);
    }

    let text = fs::read_to_string(snapshot_path)?;
    match serde_json::from_str(&text) {
        Ok(snapshot) => Ok(Some(snapshot)),
        Err(error) => {
            warnings.push(format!(
                "compatibility snapshot at {} is invalid and will be replaced: {}",
                snapshot_path.display(),
                error
            ));
            Ok(None)
        }
    }
}
