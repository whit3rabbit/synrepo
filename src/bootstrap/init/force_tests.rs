use super::{bootstrap, bootstrap_with_force};
use crate::bootstrap::BootstrapHealth;
use crate::config::Config;
use crate::store::compatibility;
use crate::store::sqlite::SqliteGraphStore;
use tempfile::tempdir;

fn isolated_home() -> (tempfile::TempDir, crate::config::test_home::HomeEnvGuard) {
    let home = tempfile::tempdir().unwrap();
    let guard = crate::config::test_home::HomeEnvGuard::redirect_to(home.path());
    (home, guard)
}

#[test]
fn bootstrap_blocks_when_snapshot_missing_and_force_recovers() {
    // Reproduces the post-`5b45f4e` upgrade trap: an existing `.synrepo/`
    // from before the schema-migration drop has graph data but no
    // `state/storage-compat.json`.
    let (_home, _home_guard) = isolated_home();
    let repo = tempdir().unwrap();
    std::fs::write(repo.path().join("README.md"), "force-init token\n").unwrap();
    std::fs::create_dir_all(repo.path().join("src")).unwrap();
    std::fs::write(repo.path().join("src/lib.rs"), "pub fn before_force() {}\n").unwrap();
    bootstrap(repo.path(), None, false).unwrap();

    let synrepo_dir = Config::synrepo_dir(repo.path());
    std::fs::remove_file(compatibility::snapshot_path(&synrepo_dir))
        .expect("remove compat snapshot");

    let blocked = bootstrap(repo.path(), None, false).unwrap_err().to_string();
    assert!(blocked.contains("Bootstrap health: blocked"));
    assert!(blocked.contains("graph"));
    assert!(blocked.contains("no compatibility snapshot exists"));

    let report = bootstrap_with_force(repo.path(), None, false, true).unwrap();
    assert!(matches!(
        report.health,
        BootstrapHealth::Healthy | BootstrapHealth::Degraded
    ));
    assert!(compatibility::snapshot_path(&synrepo_dir).exists());
    let store = SqliteGraphStore::open_existing(&synrepo_dir.join("graph")).unwrap();
    assert!(store.persisted_stats().unwrap().file_nodes >= 1);
}
