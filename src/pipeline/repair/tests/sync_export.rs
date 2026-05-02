use tempfile::tempdir;

use super::support::setup_repo_for_sync;
use crate::{
    config::Config,
    pipeline::{
        export::{load_manifest, ExportFormat, ExportManifest, MANIFEST_FILENAME},
        repair::{execute_sync, RepairSurface, SyncOptions},
    },
    store::{compatibility::GRAPH_FORMAT_VERSION, sqlite::SqliteGraphStore},
};

#[test]
fn sync_regenerates_stale_graph_exports_with_same_format() {
    for (format, expected_file) in [
        (ExportFormat::GraphJson, "graph.json"),
        (ExportFormat::GraphHtml, "graph.html"),
    ] {
        let dir = tempdir().unwrap();
        let (repo, synrepo_dir) = setup_repo_for_sync(&dir);
        let _graph = SqliteGraphStore::open(&synrepo_dir.join("graph")).unwrap();
        write_stale_export_manifest(&repo, format);

        let summary = execute_sync(
            &repo,
            &synrepo_dir,
            &Config::default(),
            SyncOptions::default(),
        )
        .unwrap();
        let repaired: Vec<_> = summary.repaired.iter().map(|f| f.surface).collect();
        assert!(
            repaired.contains(&RepairSurface::ExportSurface),
            "stale graph export should be repaired for {format:?}; repaired={repaired:?}"
        );

        let manifest = load_manifest(&repo, &Config::default()).expect("manifest should reload");
        assert_eq!(manifest.format, format);
        assert_ne!(manifest.last_reconcile_at, "stale-epoch");
        assert!(
            repo.join("synrepo-context").join(expected_file).exists(),
            "sync should regenerate {expected_file} for {format:?}"
        );
    }
}

fn write_stale_export_manifest(repo: &std::path::Path, format: ExportFormat) {
    let export_dir = repo.join("synrepo-context");
    std::fs::create_dir_all(&export_dir).unwrap();
    let manifest = ExportManifest {
        graph_schema_version: GRAPH_FORMAT_VERSION,
        last_reconcile_at: "stale-epoch".to_string(),
        format,
        budget: "normal".to_string(),
        generated_at: "2026-01-01T00:00:00Z".to_string(),
    };
    std::fs::write(
        export_dir.join(MANIFEST_FILENAME),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();
}
