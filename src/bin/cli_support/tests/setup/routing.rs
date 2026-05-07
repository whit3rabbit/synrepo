use std::fs;

use tempfile::tempdir;

use crate::cli_support::setup_cmd::{execute_setup_plan, init_entry_mode, InitEntryMode};
use crate::cli_support::tests::support::canonicalize_no_verbatim;
use synrepo::config::{Config, Mode};
use synrepo::tui::SetupPlan;

fn redirect_home() -> (
    synrepo::test_support::GlobalTestLock,
    tempfile::TempDir,
    synrepo::config::test_home::HomeEnvGuard,
) {
    let lock =
        synrepo::test_support::global_test_lock(synrepo::config::test_home::HOME_ENV_TEST_LOCK);
    let home = tempdir().unwrap();
    let canonical_home = canonicalize_no_verbatim(home.path());
    let guard = synrepo::config::test_home::HomeEnvGuard::redirect_to(&canonical_home);
    (lock, home, guard)
}

#[test]
fn uninitialized_tty_no_flag_init_routes_to_guided_setup() {
    let repo = tempdir().unwrap();

    let mode = init_entry_mode(repo.path(), false, false, false, true);

    assert_eq!(mode, InitEntryMode::GuidedSetup);
}

#[test]
fn non_tty_init_stays_raw() {
    let repo = tempdir().unwrap();

    let mode = init_entry_mode(repo.path(), false, false, false, false);

    assert_eq!(mode, InitEntryMode::RawInit);
}

#[test]
fn flagged_init_stays_raw_even_in_tty() {
    let repo = tempdir().unwrap();

    let cases = [
        (true, false, false),
        (false, true, false),
        (false, false, true),
    ];

    for (has_mode_flag, gitignore, force) in cases {
        let mode = init_entry_mode(repo.path(), has_mode_flag, gitignore, force, true);
        assert_eq!(
            mode,
            InitEntryMode::RawInit,
            "flags must keep init scriptable: {has_mode_flag:?} {gitignore:?} {force:?}"
        );
    }
}

#[test]
fn ready_repo_init_stays_raw() {
    let repo = tempdir().unwrap();
    synrepo::bootstrap::bootstrap(repo.path(), None, false).unwrap();

    let mode = init_entry_mode(repo.path(), false, false, false, true);

    assert_eq!(mode, InitEntryMode::RawInit);
}

#[test]
fn partial_repo_init_stays_raw() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join(".synrepo")).unwrap();

    let mode = init_entry_mode(repo.path(), false, false, false, true);

    assert_eq!(mode, InitEntryMode::RawInit);
}

#[test]
fn wizard_plan_execution_registers_project() {
    let (_lock, _home, _guard) = redirect_home();
    let repo = tempdir().unwrap();
    fs::write(repo.path().join("README.md"), "setup registry test\n").unwrap();

    execute_setup_plan(
        repo.path(),
        SetupPlan {
            mode: Mode::Auto,
            target: None,
            enable_embeddings: false,
            explain: None,
            reconcile_after: true,
        },
    )
    .unwrap();

    assert!(synrepo::registry::get(repo.path()).unwrap().is_some());
}

#[test]
fn wizard_plan_execution_persists_embeddings_opt_in() {
    let (_lock, _home, _guard) = redirect_home();
    let repo = tempdir().unwrap();
    fs::write(repo.path().join("README.md"), "setup embeddings test\n").unwrap();

    execute_setup_plan(
        repo.path(),
        SetupPlan {
            mode: Mode::Auto,
            target: None,
            enable_embeddings: true,
            explain: None,
            reconcile_after: true,
        },
    )
    .unwrap();

    let config = Config::load(repo.path()).unwrap();
    assert!(config.enable_semantic_triage);
    assert!(fs::read_to_string(repo.path().join(".synrepo/config.toml"))
        .unwrap()
        .contains("enable_semantic_triage = true"));
}

#[test]
#[cfg(feature = "semantic-triage")]
fn setup_init_with_semantic_triage_does_not_build_vectors_index() {
    let (_lock, _home, _guard) = redirect_home();
    let repo = tempdir().unwrap();
    fs::write(
        repo.path().join("README.md"),
        "# Test Concept\n\nA setup concept for embedding.\n",
    )
    .unwrap();

    execute_setup_plan(
        repo.path(),
        SetupPlan {
            mode: Mode::Auto,
            target: None,
            enable_embeddings: true,
            explain: None,
            reconcile_after: true,
        },
    )
    .unwrap();

    let vectors_dir = repo.path().join(".synrepo/index/vectors");
    assert!(
        !vectors_dir.join("index.bin").exists(),
        "setup must not build embeddings implicitly"
    );
}
