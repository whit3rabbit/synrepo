use super::*;
use std::fs;
use tempfile::tempdir;

#[test]
fn load_missing_file_returns_error() {
    // Config::load falls back to ~/.synrepo/config.toml when the repo-local
    // file is missing; redirect HOME under the shared lock so the user's
    // real global config can't satisfy the load and mask NotInitialized.
    let _lock = crate::test_support::global_test_lock(super::test_home::HOME_ENV_TEST_LOCK);
    let home = tempdir().unwrap();
    let _home_guard = super::test_home::HomeEnvGuard::redirect_to(home.path());

    let dir = tempdir().unwrap();
    let err = Config::load(dir.path()).unwrap_err();
    assert!(matches!(err, crate::Error::NotInitialized(_)));
}

#[test]
fn load_valid_file_overrides_defaults() {
    let _lock = crate::test_support::global_test_lock(super::test_home::HOME_ENV_TEST_LOCK);
    let home = tempdir().unwrap();
    let _home_guard = super::test_home::HomeEnvGuard::redirect_to(home.path());

    let dir = tempdir().unwrap();
    let synrepo_dir = Config::synrepo_dir(dir.path());
    fs::create_dir_all(&synrepo_dir).unwrap();

    let custom_toml = r#"
        mode = "curated"
        roots = ["src"]
        git_commit_depth = 100
        "#;
    fs::write(synrepo_dir.join("config.toml"), custom_toml).unwrap();

    let config = Config::load(dir.path()).unwrap();

    assert_eq!(config.mode, Mode::Curated);
    assert_eq!(config.roots, vec!["src".to_string()]);
    assert_eq!(config.git_commit_depth, 100);

    // Ensure defaults are kept for unmentioned fields
    assert_eq!(config.max_file_size_bytes, 1024 * 1024);
}

#[test]
fn cross_link_fields_round_trip_through_toml() {
    let _lock = crate::test_support::global_test_lock(super::test_home::HOME_ENV_TEST_LOCK);
    let home = tempdir().unwrap();
    let _home_guard = super::test_home::HomeEnvGuard::redirect_to(home.path());

    let dir = tempdir().unwrap();
    let synrepo_dir = Config::synrepo_dir(dir.path());
    fs::create_dir_all(&synrepo_dir).unwrap();

    let custom_toml = r#"
        cross_link_cost_limit = 42
        [cross_link_confidence_thresholds]
        high = 0.9
        review_queue = 0.55
    "#;
    fs::write(synrepo_dir.join("config.toml"), custom_toml).unwrap();

    let config = Config::load(dir.path()).unwrap();
    assert_eq!(config.cross_link_cost_limit, 42);
    assert!((config.cross_link_confidence_thresholds.high - 0.9).abs() < 1e-6);
    assert!((config.cross_link_confidence_thresholds.review_queue - 0.55).abs() < 1e-6);

    // Defaults kick in when the TOML omits the cross-link keys.
    let default = Config::default();
    assert_eq!(default.cross_link_cost_limit, 200);
    assert!((default.cross_link_confidence_thresholds.high - 0.85).abs() < 1e-6);
}

#[test]
fn load_invalid_toml_returns_error() {
    let _lock = crate::test_support::global_test_lock(super::test_home::HOME_ENV_TEST_LOCK);
    let home = tempdir().unwrap();
    let _home_guard = super::test_home::HomeEnvGuard::redirect_to(home.path());

    let dir = tempdir().unwrap();
    let synrepo_dir = Config::synrepo_dir(dir.path());
    fs::create_dir_all(&synrepo_dir).unwrap();

    fs::write(synrepo_dir.join("config.toml"), "mode = [").unwrap();

    let err = Config::load(dir.path()).unwrap_err();
    assert!(err.to_string().starts_with("config error:"));
}

#[test]
fn merge_overrides_fields() {
    let mut base = Config::default();
    base.git_commit_depth = 100;
    base.mode = Mode::Auto;

    let mut other = Config::default();
    other.git_commit_depth = 200;
    other.mode = Mode::Curated;

    base.merge(other);

    assert_eq!(base.git_commit_depth, 200);
    assert_eq!(base.mode, Mode::Curated);
}

#[test]
fn merge_preserves_unmodified_fields() {
    let mut base = Config::default();
    base.commentary_cost_limit = 1000;

    let other = Config::default(); // default commentary_cost_limit is 5000
    base.merge(other);

    assert_eq!(base.commentary_cost_limit, 1000);
}

#[test]
fn load_merges_global_and_local() {
    let _lock = crate::test_support::global_test_lock(super::test_home::HOME_ENV_TEST_LOCK);
    let home = tempdir().unwrap();
    let repo = tempdir().unwrap();
    let _home_guard = super::test_home::HomeEnvGuard::redirect_to(home.path());
    std::fs::create_dir_all(home.path().join(".synrepo")).unwrap();
    std::fs::create_dir_all(repo.path().join(".synrepo")).unwrap();

    let global_toml = r#"
        mode = "curated"
        [explain]
        enabled = true
        provider = "anthropic"
        anthropic_api_key = "global-anthropic"
        local_endpoint = "http://global-llm:11434/api/chat"
    "#;
    std::fs::write(home.path().join(".synrepo/config.toml"), global_toml).unwrap();

    let local_toml = r#"
        mode = "auto"
        [explain]
        provider = "openai"
    "#;
    std::fs::write(repo.path().join(".synrepo/config.toml"), local_toml).unwrap();

    // Config::load should merge: mode is auto (local wins), explain enabled is true (global preserved), explain provider is openai (local wins)
    let config = Config::load(repo.path()).expect("load must succeed");

    assert_eq!(config.mode, Mode::Auto);
    assert!(config.explain.enabled);
    assert_eq!(config.explain.provider.as_deref(), Some("openai"));
    assert_eq!(
        config.explain.anthropic_api_key.as_deref(),
        Some("global-anthropic")
    );
    assert_eq!(
        config.explain.local_endpoint.as_deref(),
        Some("http://global-llm:11434/api/chat")
    );
}
