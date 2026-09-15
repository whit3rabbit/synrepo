use tempfile::TempDir;

use super::super::Config;

struct ConfigLayers {
    _home_guard: super::super::test_home::HomeEnvGuard,
    repo: TempDir,
    _home: TempDir,
    _lock: crate::test_support::GlobalTestLock,
}

fn config_layers(global: &str, local: &str) -> ConfigLayers {
    let lock = crate::test_support::global_test_lock(super::super::test_home::HOME_ENV_TEST_LOCK);
    let home = tempfile::tempdir().unwrap();
    let repo = tempfile::tempdir().unwrap();
    let home_guard = super::super::test_home::HomeEnvGuard::redirect_to(home.path());
    std::fs::create_dir_all(home.path().join(".synrepo")).unwrap();
    std::fs::create_dir_all(repo.path().join(".synrepo")).unwrap();
    std::fs::write(home.path().join(".synrepo/config.toml"), global).unwrap();
    std::fs::write(repo.path().join(".synrepo/config.toml"), local).unwrap();
    ConfigLayers {
        _home_guard: home_guard,
        repo,
        _home: home,
        _lock: lock,
    }
}

#[test]
fn local_keepalive_zero_overrides_global_positive_value() {
    let layers = config_layers(
        "reconcile_keepalive_seconds = 120\n",
        "reconcile_keepalive_seconds = 0\n",
    );

    let config = Config::load(layers.repo.path()).unwrap();

    assert_eq!(config.reconcile_keepalive_seconds, 0);
}

#[test]
fn missing_local_keepalive_inherits_global_positive_value() {
    let layers = config_layers("reconcile_keepalive_seconds = 120\n", "mode = \"auto\"\n");

    let config = Config::load(layers.repo.path()).unwrap();

    assert_eq!(config.reconcile_keepalive_seconds, 120);
}
