#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tempfile::tempdir;

use crate::cli_support::commands::{step_setup_external_syntext_with_program, StepOutcome};

#[test]
fn step_setup_external_syntext_indexes_and_records_gitignore_line() {
    let _home_lock =
        synrepo::test_support::global_test_lock(synrepo::config::test_home::HOME_ENV_TEST_LOCK);
    let home = tempdir().unwrap();
    let _home_guard = synrepo::config::test_home::HomeEnvGuard::redirect_to(home.path());
    let repo = tempdir().unwrap();
    let fake_st = write_fake_st(repo.path(), true, 0);

    let outcome =
        step_setup_external_syntext_with_program(repo.path(), &fake_st, Duration::from_secs(2))
            .unwrap();

    assert_eq!(outcome, StepOutcome::Applied);
    assert!(repo.path().join(".syntext/manifest.json").exists());
    let gitignore = fs::read_to_string(repo.path().join(".gitignore")).unwrap();
    assert!(gitignore.lines().any(|line| line.trim() == ".syntext/"));
    let args = fs::read_to_string(fake_st.with_file_name("st.args")).unwrap();
    assert_eq!(
        args,
        format!(
            "index\n--quiet\n--repo-root\n{}\n--index-dir\n{}\n",
            repo.path().display(),
            repo.path().join(".syntext").display()
        )
    );
    let entry = synrepo::registry::get(repo.path()).unwrap().unwrap();
    assert!(entry.syntext_gitignore_entry_added);
}

#[test]
fn step_setup_external_syntext_failed_index_is_manual_followup() {
    let _home_lock =
        synrepo::test_support::global_test_lock(synrepo::config::test_home::HOME_ENV_TEST_LOCK);
    let home = tempdir().unwrap();
    let _home_guard = synrepo::config::test_home::HomeEnvGuard::redirect_to(home.path());
    let repo = tempdir().unwrap();
    let fake_st = write_fake_st(repo.path(), false, 42);

    let outcome =
        step_setup_external_syntext_with_program(repo.path(), &fake_st, Duration::from_secs(2))
            .unwrap();

    assert_eq!(outcome, StepOutcome::NotAutomated);
    assert!(!repo.path().join(".gitignore").exists());
    assert!(synrepo::registry::get(repo.path()).unwrap().is_none());
}

fn write_fake_st(repo: &Path, create_manifest: bool, exit_code: i32) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let bin_dir = repo.join(".fake-bin");
    fs::create_dir_all(&bin_dir).unwrap();
    let fake_st = bin_dir.join("st");
    let manifest_block = if create_manifest {
        "mkdir -p \"$6\"\nprintf '{}' > \"$6/manifest.json\"\n"
    } else {
        ""
    };
    fs::write(
        &fake_st,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$0.args\"\n{manifest_block}exit {exit_code}\n"
        ),
    )
    .unwrap();
    let mut permissions = fs::metadata(&fake_st).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_st, permissions).unwrap();
    fake_st
}
