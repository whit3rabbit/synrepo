use super::bootstrap;
use crate::bootstrap::BootstrapHealth;
use crate::config::{Config, Mode};
use crate::store::compatibility::{self, StoreId};
use crate::store::sqlite::SqliteGraphStore;
use crate::structure::graph::GraphStore;
use tempfile::tempdir;

#[test]
fn bootstrap_fresh_repo_reports_healthy_summary() {
    let repo = tempdir().unwrap();
    std::fs::write(repo.path().join("README.md"), "fresh token\n").unwrap();

    let report = bootstrap(repo.path(), None).unwrap();
    let rendered = report.render();

    assert_eq!(report.health, BootstrapHealth::Healthy);
    assert_eq!(report.mode, Mode::Auto);
    assert!(rendered.contains("Bootstrap health: healthy"));
    assert!(rendered.contains("Mode: Auto"));
    assert!(rendered.contains("Mode guidance: no rationale markdown was found"));
    assert!(rendered.contains("Runtime path:"));
    assert!(rendered.contains("Substrate: built initial index"));
    assert!(rendered.contains("Next: run `synrepo search <query>`"));
    assert!(compatibility::snapshot_path(&Config::synrepo_dir(repo.path())).exists());
}

#[test]
fn bootstrap_selects_curated_when_rationale_markdown_exists() {
    let repo = tempdir().unwrap();
    let adr_dir = repo.path().join("docs/adr");
    std::fs::create_dir_all(&adr_dir).unwrap();
    std::fs::write(adr_dir.join("0001-record.md"), "# Decision\n").unwrap();
    std::fs::write(repo.path().join("README.md"), "curated token\n").unwrap();

    let report = bootstrap(repo.path(), None).unwrap();
    let rendered = report.render();
    let config = Config::load(repo.path()).unwrap();

    assert_eq!(report.mode, Mode::Curated);
    assert_eq!(config.mode, Mode::Curated);
    assert!(rendered.contains("Mode guidance: repository inspection selected Curated"));
    assert!(rendered.contains("docs/adr"));
}

#[test]
fn bootstrap_rerun_refreshes_existing_runtime() {
    let repo = tempdir().unwrap();
    std::fs::write(repo.path().join("README.md"), "before refresh\n").unwrap();
    bootstrap(repo.path(), None).unwrap();

    std::fs::write(repo.path().join("README.md"), "after refresh token\n").unwrap();

    let report = bootstrap(repo.path(), None).unwrap();
    let matches = crate::substrate::search(
        &Config::load(repo.path()).unwrap(),
        repo.path(),
        "after refresh token",
    )
    .unwrap();

    assert_eq!(report.health, BootstrapHealth::Healthy);
    assert!(report.substrate_status.contains("refreshed existing index"));
    assert_eq!(matches.len(), 1);
}

#[test]
fn bootstrap_repairs_partial_runtime_and_reports_degraded() {
    let repo = tempdir().unwrap();
    let synrepo_dir = Config::synrepo_dir(repo.path());
    std::fs::create_dir_all(&synrepo_dir).unwrap();
    std::fs::write(repo.path().join("README.md"), "repair token\n").unwrap();

    let report = bootstrap(repo.path(), None).unwrap();
    let rendered = report.render();

    assert_eq!(report.health, BootstrapHealth::Degraded);
    assert!(rendered.contains("Bootstrap health: degraded"));
    assert!(rendered.contains("repaired runtime state and rebuilt index"));
    assert!(synrepo_dir.join("index/manifest.json").exists());
}

#[test]
fn bootstrap_reports_graph_sensitive_config_drift_without_blocking() {
    let repo = tempdir().unwrap();
    std::fs::write(repo.path().join("README.md"), "compat token\n").unwrap();
    bootstrap(repo.path(), None).unwrap();

    let updated = Config {
        concept_directories: vec![
            "docs/concepts".to_string(),
            "docs/adr".to_string(),
            "docs/decisions".to_string(),
            "architecture/decisions".to_string(),
        ],
        ..Config::load(repo.path()).unwrap()
    };
    std::fs::write(
        Config::synrepo_dir(repo.path()).join("config.toml"),
        toml::to_string_pretty(&updated).unwrap(),
    )
    .unwrap();

    let report = bootstrap(repo.path(), None).unwrap();
    let rendered = report.render();

    assert!(rendered.contains("Compatibility:"));
    assert!(rendered.contains("concept_directories"));
}

#[test]
fn bootstrap_blocks_on_invalid_existing_config() {
    let repo = tempdir().unwrap();
    let synrepo_dir = Config::synrepo_dir(repo.path());
    std::fs::create_dir_all(&synrepo_dir).unwrap();
    std::fs::write(synrepo_dir.join("config.toml"), "mode = [not valid").unwrap();

    let error = bootstrap(repo.path(), None).unwrap_err().to_string();

    assert!(error.contains("Bootstrap health: blocked"));
    assert!(error.contains("invalid existing config"));
}

#[test]
fn bootstrap_explicit_mode_overrides_existing_config_on_refresh() {
    let repo = tempdir().unwrap();
    std::fs::write(repo.path().join("README.md"), "mode token\n").unwrap();
    bootstrap(repo.path(), Some(Mode::Curated)).unwrap();

    let report = bootstrap(repo.path(), Some(Mode::Auto)).unwrap();
    let config = Config::load(repo.path()).unwrap();

    assert_eq!(report.mode, Mode::Auto);
    assert_eq!(config.mode, Mode::Auto);
}

#[test]
fn bootstrap_honors_explicit_auto_with_curated_recommendation() {
    let repo = tempdir().unwrap();
    let adr_dir = repo.path().join("docs/adr");
    std::fs::create_dir_all(&adr_dir).unwrap();
    std::fs::write(adr_dir.join("0002-architecture.md"), "# Architecture\n").unwrap();
    std::fs::write(repo.path().join("README.md"), "explicit token\n").unwrap();

    let report = bootstrap(repo.path(), Some(Mode::Auto)).unwrap();
    let rendered = report.render();
    let config = Config::load(repo.path()).unwrap();

    assert_eq!(report.mode, Mode::Auto);
    assert_eq!(config.mode, Mode::Auto);
    assert!(rendered.contains("Mode guidance: repository inspection suggests Curated"));
    assert!(rendered.contains("keeping explicit Auto"));
}

#[test]
fn bootstrap_blocks_on_newer_graph_store_version() {
    let repo = tempdir().unwrap();
    std::fs::write(repo.path().join("README.md"), "graph token\n").unwrap();
    bootstrap(repo.path(), None).unwrap();

    let synrepo_dir = Config::synrepo_dir(repo.path());
    std::fs::write(synrepo_dir.join("graph/nodes.db"), "db").unwrap();
    let mut snapshot =
        compatibility::write_runtime_snapshot(&synrepo_dir, &Config::load(repo.path()).unwrap())
            .unwrap();
    snapshot
        .store_format_versions
        .insert(StoreId::Graph.as_str().to_string(), 2);
    std::fs::write(
        compatibility::snapshot_path(&synrepo_dir),
        serde_json::to_vec_pretty(&snapshot).unwrap(),
    )
    .unwrap();

    let error = bootstrap(repo.path(), None).unwrap_err().to_string();

    assert!(error.contains("Bootstrap health: blocked"));
    assert!(error.contains("graph"));
    assert!(error.contains("block"));
}

#[test]
fn bootstrap_fresh_init_materializes_graph_with_code_symbols() {
    let repo = tempdir().unwrap();
    std::fs::create_dir_all(repo.path().join("src")).unwrap();
    std::fs::write(
        repo.path().join("src/lib.rs"),
        "pub fn hello() {}\npub struct World;\n",
    )
    .unwrap();

    let report = bootstrap(repo.path(), None).unwrap();

    let graph_dir = Config::synrepo_dir(repo.path()).join("graph");
    let store = SqliteGraphStore::open_existing(&graph_dir).unwrap();
    let stats = store.persisted_stats().unwrap();

    assert_eq!(report.health, BootstrapHealth::Healthy);
    assert!(report.graph_status.contains("file nodes"));
    assert!(
        stats.file_nodes >= 1,
        "at least one file node for src/lib.rs"
    );
    assert!(stats.symbol_nodes >= 2, "at least hello and World symbols");
    assert!(
        stats.total_edges >= 2,
        "at least defines edges for each symbol"
    );
}

#[test]
fn bootstrap_rerun_refreshes_graph_on_content_change() {
    let repo = tempdir().unwrap();
    std::fs::create_dir_all(repo.path().join("src")).unwrap();
    std::fs::write(repo.path().join("src/lib.rs"), "pub fn before() {}\n").unwrap();
    bootstrap(repo.path(), None).unwrap();

    std::fs::write(repo.path().join("src/lib.rs"), "pub fn after() {}\n").unwrap();
    bootstrap(repo.path(), None).unwrap();

    let graph_dir = Config::synrepo_dir(repo.path()).join("graph");
    let store = SqliteGraphStore::open_existing(&graph_dir).unwrap();
    let paths = store.all_file_paths().unwrap();

    assert!(
        paths.iter().any(|(path, _)| path == "src/lib.rs"),
        "file node must survive refresh"
    );
}
