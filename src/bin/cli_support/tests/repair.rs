use std::fs;

use tempfile::tempdir;

use crate::cli_support::repair_cmd::execute_repair_plan;
use synrepo::config::Config;
use synrepo::store::compatibility;
use synrepo::tui::RepairPlan;

fn isolated_home() -> (
    tempfile::TempDir,
    synrepo::config::test_home::HomeEnvGuard,
    synrepo::test_support::GlobalTestLock,
) {
    let lock =
        synrepo::test_support::global_test_lock(synrepo::config::test_home::HOME_ENV_TEST_LOCK);
    let home = tempdir().unwrap();
    let guard = synrepo::config::test_home::HomeEnvGuard::redirect_to(home.path());
    (home, guard, lock)
}

#[test]
fn repair_plan_recreate_runtime_recovers_missing_compat_snapshot() {
    let (_home, _guard, _lock) = isolated_home();
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(repo.path().join("src/lib.rs"), "pub fn repair_force() {}\n").unwrap();
    synrepo::bootstrap::bootstrap(repo.path(), None, false).unwrap();

    let synrepo_dir = Config::synrepo_dir(repo.path());
    fs::remove_file(compatibility::snapshot_path(&synrepo_dir)).unwrap();

    execute_repair_plan(
        repo.path(),
        RepairPlan {
            recreate_runtime: true,
            ..RepairPlan::default()
        },
    )
    .unwrap();

    assert!(compatibility::snapshot_path(&synrepo_dir).exists());
    assert!(synrepo_dir.join("graph/nodes.db").exists());
}
