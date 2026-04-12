use super::super::commands::search;
use synrepo::bootstrap::bootstrap;
use synrepo::config::Config;
use synrepo::pipeline::writer::{writer_lock_path, WriterOwnership};
use syntext::SearchOptions;
use tempfile::tempdir;

#[test]
fn search_requires_rebuild_when_index_sensitive_config_changes() {
    let repo = tempdir().unwrap();
    std::fs::write(repo.path().join("README.md"), "search token\n").unwrap();
    bootstrap(repo.path(), None).unwrap();

    let updated = Config {
        roots: vec!["src".to_string()],
        ..Config::load(repo.path()).unwrap()
    };
    std::fs::write(
        Config::synrepo_dir(repo.path()).join("config.toml"),
        toml::to_string_pretty(&updated).unwrap(),
    )
    .unwrap();

    let error = search(repo.path(), "search token", SearchOptions::default())
        .unwrap_err()
        .to_string();

    assert!(error.contains("Storage compatibility"));
    assert!(error.contains("requires rebuild"));
}

#[test]
fn search_refuses_to_race_the_writer_lock() {
    let repo = tempdir().unwrap();
    std::fs::write(repo.path().join("README.md"), "search token\n").unwrap();
    bootstrap(repo.path(), None).unwrap();

    let synrepo_dir = Config::synrepo_dir(repo.path());
    std::fs::create_dir_all(synrepo_dir.join("state")).unwrap();
    let mut child = std::process::Command::new("sleep")
        .arg("5")
        .spawn()
        .unwrap();
    std::fs::write(
        writer_lock_path(&synrepo_dir),
        serde_json::to_string(&WriterOwnership {
            pid: child.id(),
            acquired_at: "now".to_string(),
        })
        .unwrap(),
    )
    .unwrap();

    let error = search(repo.path(), "search token", SearchOptions::default())
        .unwrap_err()
        .to_string();

    assert!(error.contains("writer lock"));
    assert!(error.contains("retry"));

    let _ = child.kill();
    let _ = child.wait();
}
