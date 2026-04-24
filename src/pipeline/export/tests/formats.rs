use tempfile::tempdir;

use super::support::init_empty_graph;
use crate::config::Config;
use crate::pipeline::export::{load_manifest, write_exports, ExportFormat, MANIFEST_FILENAME};
use crate::surface::card::Budget;

#[test]
fn export_produces_markdown_files() {
    let repo = tempdir().unwrap();
    let synrepo_dir = repo.path().join(".synrepo");
    init_empty_graph(&synrepo_dir).unwrap();

    let config = Config {
        export_dir: "test-export".to_string(),
        ..Config::default()
    };

    write_exports(
        repo.path(),
        &synrepo_dir,
        &config,
        ExportFormat::Markdown,
        Budget::Normal,
        true, // --commit: suppress gitignore insertion
    )
    .unwrap();

    let export_dir = repo.path().join("test-export");
    assert!(
        export_dir.join("files.md").exists(),
        "files.md should exist"
    );
    assert!(
        export_dir.join("symbols.md").exists(),
        "symbols.md should exist"
    );
    assert!(
        export_dir.join("decisions.md").exists(),
        "decisions.md should exist"
    );
    assert!(
        export_dir.join(MANIFEST_FILENAME).exists(),
        ".export-manifest.json should exist"
    );
}

#[test]
fn export_produces_json_file() {
    let repo = tempdir().unwrap();
    let synrepo_dir = repo.path().join(".synrepo");
    init_empty_graph(&synrepo_dir).unwrap();

    let config = Config {
        export_dir: "test-export-json".to_string(),
        ..Config::default()
    };

    write_exports(
        repo.path(),
        &synrepo_dir,
        &config,
        ExportFormat::Json,
        Budget::Normal,
        true,
    )
    .unwrap();

    let export_dir = repo.path().join("test-export-json");
    assert!(
        export_dir.join("index.json").exists(),
        "index.json should exist"
    );
}

#[test]
fn manifest_records_correct_format_and_budget() {
    let repo = tempdir().unwrap();
    let synrepo_dir = repo.path().join(".synrepo");
    init_empty_graph(&synrepo_dir).unwrap();

    let config = Config {
        export_dir: "test-export-manifest".to_string(),
        ..Config::default()
    };

    write_exports(
        repo.path(),
        &synrepo_dir,
        &config,
        ExportFormat::Markdown,
        Budget::Deep,
        true,
    )
    .unwrap();

    let manifest = load_manifest(repo.path(), &config).expect("manifest should load");
    assert_eq!(manifest.format, ExportFormat::Markdown);
    assert_eq!(manifest.budget, "deep");
    assert!(!manifest.generated_at.is_empty());
}

#[test]
fn deep_flag_uses_deep_budget() {
    let repo = tempdir().unwrap();
    let synrepo_dir = repo.path().join(".synrepo");
    init_empty_graph(&synrepo_dir).unwrap();

    let config = Config {
        export_dir: "test-export-deep".to_string(),
        ..Config::default()
    };

    let result = write_exports(
        repo.path(),
        &synrepo_dir,
        &config,
        ExportFormat::Markdown,
        Budget::Deep,
        true,
    )
    .unwrap();

    assert_eq!(result.manifest.budget, "deep");
}
