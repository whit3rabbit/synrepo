//! Snapshot I/O for the `.synrepo/` compatibility state.

use crate::config::Config;
use std::{
    fs,
    path::{Path, PathBuf},
};

use super::types::{ConfigFingerprints, RuntimeCompatibilitySnapshot, StoreId};

/// Ensure the expected runtime layout exists under `.synrepo/`.
///
/// Returns `true` if any directories were created.
pub fn ensure_runtime_layout(synrepo_dir: &Path) -> crate::Result<bool> {
    let expected_directories = [
        synrepo_dir.to_path_buf(),
        synrepo_dir.join("graph"),
        synrepo_dir.join("overlay"),
        synrepo_dir.join("index"),
        synrepo_dir.join("embeddings"),
        synrepo_dir.join("cache/llm-responses"),
        synrepo_dir.join("state"),
    ];

    let mut any_missing = false;
    for directory in expected_directories {
        if !directory.exists() {
            any_missing = true;
            fs::create_dir_all(&directory)?;
        }
    }

    Ok(any_missing)
}

/// Write the current compatibility snapshot to `.synrepo/state/`.
pub fn write_runtime_snapshot(
    synrepo_dir: &Path,
    config: &Config,
) -> crate::Result<RuntimeCompatibilitySnapshot> {
    let snapshot = RuntimeCompatibilitySnapshot {
        snapshot_version: super::SNAPSHOT_VERSION,
        store_format_versions: StoreId::ALL
            .into_iter()
            .map(|store_id| {
                (
                    store_id.as_str().to_string(),
                    store_id.expected_format_version(),
                )
            })
            .collect(),
        config_fingerprints: ConfigFingerprints::from_config(config),
    };

    fs::create_dir_all(synrepo_dir.join("state"))?;
    let json = serde_json::to_vec_pretty(&snapshot)
        .map_err(|error| crate::Error::Other(anyhow::anyhow!(error)))?;
    fs::write(snapshot_path(synrepo_dir), json)?;
    Ok(snapshot)
}

/// Return the compatibility snapshot path for this runtime.
pub fn snapshot_path(synrepo_dir: &Path) -> PathBuf {
    synrepo_dir.join("state").join(super::SNAPSHOT_FILENAME)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::compatibility::StoreId;
    use tempfile::tempdir;

    #[test]
    fn write_runtime_snapshot_records_expected_versions() {
        let repo = tempdir().unwrap();
        let synrepo_dir = repo.path().join(".synrepo");
        let snapshot =
            write_runtime_snapshot(&synrepo_dir, &crate::config::Config::default()).unwrap();

        assert_eq!(snapshot.snapshot_version, super::super::SNAPSHOT_VERSION);
        assert_eq!(
            snapshot.store_format_versions.get(StoreId::Index.as_str()),
            Some(&super::super::STORE_FORMAT_VERSION)
        );
        assert!(snapshot_path(&synrepo_dir).exists());
    }
}
