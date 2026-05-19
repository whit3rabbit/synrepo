use super::*;
use std::{fs, path::Path, time::Duration};

use tempfile::tempdir;

#[test]
fn external_syntext_detection_requires_manifest() {
    let repo = tempdir().unwrap();

    assert!(!external_syntext_index_exists(repo.path()));

    fs::create_dir_all(repo.path().join(EXTERNAL_SYNTEXT_DIR)).unwrap();
    assert!(!external_syntext_index_exists(repo.path()));

    fs::write(
        repo.path()
            .join(EXTERNAL_SYNTEXT_DIR)
            .join(EXTERNAL_SYNTEXT_MANIFEST),
        "{}",
    )
    .unwrap();
    assert!(external_syntext_index_exists(repo.path()));
}

#[test]
fn external_syntext_update_skips_missing_manifest() {
    let repo = tempdir().unwrap();
    let missing_program = repo.path().join("missing-st");

    let report = sync_external_syntext_index_with_program(
        repo.path(),
        &missing_program,
        Duration::from_secs(1),
    )
    .unwrap();

    assert_eq!(report, ExternalSyntextSync::Skipped);
}

#[test]
fn root_gitignore_syntext_entry_is_appended_once() {
    let repo = tempdir().unwrap();

    assert!(!root_gitignore_contains_syntext(repo.path()).unwrap());
    assert!(ensure_root_gitignore_entry(repo.path()).unwrap());
    assert!(!ensure_root_gitignore_entry(repo.path()).unwrap());
    assert!(root_gitignore_contains_syntext(repo.path()).unwrap());

    let raw = fs::read_to_string(repo.path().join(".gitignore")).unwrap();
    assert_eq!(
        raw.lines()
            .filter(|line| line.trim() == EXTERNAL_SYNTEXT_GITIGNORE_ENTRY)
            .count(),
        1
    );
}

#[test]
fn root_gitignore_existing_syntext_variants_are_respected() {
    for entry in [
        ".syntext",
        "/.syntext",
        "/.syntext/",
        ".syntext/**",
        "/.syntext/**",
    ] {
        let repo = tempdir().unwrap();
        fs::write(
            repo.path().join(".gitignore"),
            format!("target/\n{entry}\n"),
        )
        .unwrap();

        assert!(root_gitignore_contains_syntext(repo.path()).unwrap());
        assert!(!ensure_root_gitignore_entry(repo.path()).unwrap());

        let raw = fs::read_to_string(repo.path().join(".gitignore")).unwrap();
        assert_eq!(raw, format!("target/\n{entry}\n"));
    }
}

#[test]
fn external_syntext_update_missing_binary_errors_when_manifest_exists() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join(EXTERNAL_SYNTEXT_DIR)).unwrap();
    fs::write(
        repo.path()
            .join(EXTERNAL_SYNTEXT_DIR)
            .join(EXTERNAL_SYNTEXT_MANIFEST),
        "{}",
    )
    .unwrap();
    let missing_program = repo.path().join("missing-st");

    let err = sync_external_syntext_index_with_program(
        repo.path(),
        &missing_program,
        Duration::from_secs(1),
    )
    .unwrap_err();
    assert!(err
        .to_string()
        .contains("unable to run external syntext command"));
}

#[cfg(unix)]
#[test]
fn external_syntext_update_invokes_st_update() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join(EXTERNAL_SYNTEXT_DIR)).unwrap();
    fs::write(
        repo.path()
            .join(EXTERNAL_SYNTEXT_DIR)
            .join(EXTERNAL_SYNTEXT_MANIFEST),
        "{}",
    )
    .unwrap();
    let fake_st = write_fake_st(repo.path(), "exit 0\n");

    let report =
        sync_external_syntext_index_with_program(repo.path(), &fake_st, Duration::from_secs(2))
            .unwrap();

    assert_eq!(report, ExternalSyntextSync::Updated);
    let args = fs::read_to_string(fake_st.with_file_name("st.args")).unwrap();
    assert_eq!(
        args,
        format!(
            "update\n--quiet\n--repo-root\n{}\n--index-dir\n{}\n",
            repo.path().display(),
            repo.path().join(EXTERNAL_SYNTEXT_DIR).display()
        )
    );
}

#[cfg(unix)]
#[test]
fn external_syntext_index_invokes_st_index() {
    let repo = tempdir().unwrap();
    let fake_st = write_fake_st(repo.path(), "exit 0\n");

    build_external_syntext_index_with_program(repo.path(), &fake_st, Duration::from_secs(2))
        .unwrap();

    let args = fs::read_to_string(fake_st.with_file_name("st.args")).unwrap();
    assert_eq!(
        args,
        format!(
            "index\n--quiet\n--repo-root\n{}\n--index-dir\n{}\n",
            repo.path().display(),
            repo.path().join(EXTERNAL_SYNTEXT_DIR).display()
        )
    );
}

#[cfg(unix)]
#[test]
fn st_available_invokes_version() {
    let repo = tempdir().unwrap();
    let fake_st = write_fake_st(repo.path(), "exit 0\n");

    assert!(st_available_with_program(&fake_st, Duration::from_secs(2)));

    let args = fs::read_to_string(fake_st.with_file_name("st.args")).unwrap();
    assert_eq!(args, "--version\n");
}

#[cfg(unix)]
fn write_fake_st(repo: &Path, trailer: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let bin_dir = repo.join(".fake-bin");
    fs::create_dir_all(&bin_dir).unwrap();
    let fake_st = bin_dir.join("st");
    fs::write(
        &fake_st,
        format!("#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$0.args\"\n{trailer}"),
    )
    .unwrap();
    let mut permissions = fs::metadata(&fake_st).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_st, permissions).unwrap();
    fake_st
}
