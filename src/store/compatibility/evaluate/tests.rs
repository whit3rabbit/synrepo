use super::*;
use crate::store::compatibility::{snapshot_path, write_runtime_snapshot};
use std::fs;
use tempfile::tempdir;

#[test]
fn missing_snapshot_requires_index_rebuild_and_cache_invalidation() {
    let repo = tempdir().unwrap();
    let synrepo_dir = repo.path().join(".synrepo");
    fs::create_dir_all(synrepo_dir.join("index")).unwrap();
    fs::create_dir_all(synrepo_dir.join("cache/llm-responses")).unwrap();
    fs::write(synrepo_dir.join("index/manifest.json"), "{}").unwrap();
    fs::write(
        synrepo_dir.join("cache/llm-responses/cached-response.json"),
        "{}",
    )
    .unwrap();

    let report = evaluate_runtime(&synrepo_dir, true, &crate::config::Config::default()).unwrap();

    assert_eq!(report.action_for(StoreId::Index), CompatAction::Rebuild);
    assert_eq!(
        report.action_for(StoreId::LlmResponsesCache),
        CompatAction::Invalidate
    );
}

#[test]
fn newer_graph_format_blocks_canonical_runtime() {
    let repo = tempdir().unwrap();
    let synrepo_dir = repo.path().join(".synrepo");
    fs::create_dir_all(synrepo_dir.join("graph")).unwrap();
    fs::write(synrepo_dir.join("graph/nodes.db"), "db").unwrap();

    let mut snapshot = RuntimeCompatibilitySnapshot {
        snapshot_version: super::super::SNAPSHOT_VERSION,
        store_format_versions: StoreId::ALL
            .into_iter()
            .map(|store_id| {
                (
                    store_id.as_str().to_string(),
                    store_id.expected_format_version(),
                )
            })
            .collect(),
        config_fingerprints: ConfigFingerprints::from_config(&crate::config::Config::default()),
    };
    snapshot.store_format_versions.insert(
        StoreId::Graph.as_str().to_string(),
        super::super::GRAPH_FORMAT_VERSION + 1,
    );
    fs::create_dir_all(synrepo_dir.join("state")).unwrap();
    fs::write(
        snapshot_path(&synrepo_dir),
        serde_json::to_vec_pretty(&snapshot).unwrap(),
    )
    .unwrap();

    let report = evaluate_runtime(&synrepo_dir, true, &crate::config::Config::default()).unwrap();

    assert_eq!(report.action_for(StoreId::Graph), CompatAction::Block);
    assert!(report.has_blocking_actions());
}

#[test]
fn cross_link_threshold_change_is_advisory_only() {
    let repo = tempdir().unwrap();
    let synrepo_dir = repo.path().join(".synrepo");
    fs::create_dir_all(synrepo_dir.join("state")).unwrap();
    fs::create_dir_all(synrepo_dir.join("graph")).unwrap();
    fs::write(synrepo_dir.join("graph/nodes.db"), "db").unwrap();
    write_runtime_snapshot(&synrepo_dir, &crate::config::Config::default()).unwrap();

    let mut config = crate::config::Config::default();
    config.cross_link_confidence_thresholds.high = 0.9;

    let report = evaluate_runtime(&synrepo_dir, true, &config).unwrap();

    // No rebuild or invalidate for any store.
    assert_eq!(report.action_for(StoreId::Graph), CompatAction::Continue);
    assert_eq!(report.action_for(StoreId::Overlay), CompatAction::Continue);
    assert_eq!(report.action_for(StoreId::Index), CompatAction::Continue);
    // Warning surfaced instead.
    assert!(report
        .warnings
        .iter()
        .any(|w| w.contains("cross_link_confidence_thresholds")));
}

#[test]
fn graph_sensitive_config_drift_warns_before_graph_exists() {
    let repo = tempdir().unwrap();
    let synrepo_dir = repo.path().join(".synrepo");
    fs::create_dir_all(synrepo_dir.join("state")).unwrap();
    write_runtime_snapshot(&synrepo_dir, &crate::config::Config::default()).unwrap();

    let mut config = crate::config::Config::default();
    config
        .concept_directories
        .push("architecture/decisions".to_string());

    let report = evaluate_runtime(&synrepo_dir, true, &config).unwrap();

    assert_eq!(report.action_for(StoreId::Graph), CompatAction::Continue);
    assert!(report
        .warnings
        .iter()
        .any(|warning| warning.contains("concept_directories")));
}

#[test]
fn legacy_global_version_treated_as_continue_for_non_graph_stores() {
    let repo = tempdir().unwrap();
    let synrepo_dir = repo.path().join(".synrepo");
    fs::create_dir_all(synrepo_dir.join("overlay")).unwrap();
    fs::create_dir_all(synrepo_dir.join("index")).unwrap();
    fs::write(synrepo_dir.join("overlay/overlay.db"), "db").unwrap();
    fs::write(synrepo_dir.join("index/manifest.json"), "{}").unwrap();

    // Simulate a legacy snapshot where all stores had the global version 2.
    let snapshot = RuntimeCompatibilitySnapshot {
        snapshot_version: super::super::SNAPSHOT_VERSION,
        store_format_versions: StoreId::ALL
            .into_iter()
            .map(|store_id| {
                (
                    store_id.as_str().to_string(),
                    super::super::LEGACY_GLOBAL_FORMAT_VERSION,
                )
            })
            .collect(),
        config_fingerprints: ConfigFingerprints::from_config(&crate::config::Config::default()),
    };
    fs::create_dir_all(synrepo_dir.join("state")).unwrap();
    fs::write(
        snapshot_path(&synrepo_dir),
        serde_json::to_vec_pretty(&snapshot).unwrap(),
    )
    .unwrap();

    let report = evaluate_runtime(&synrepo_dir, true, &crate::config::Config::default()).unwrap();

    // Graph should Continue (its expected version IS the legacy global).
    assert_eq!(report.action_for(StoreId::Graph), CompatAction::Continue);
    // Non-graph stores with stored=2 and expected=1 must also Continue,
    // not Invalidate or Block.
    assert_eq!(report.action_for(StoreId::Overlay), CompatAction::Continue);
    assert_eq!(report.action_for(StoreId::Index), CompatAction::Continue);
}
