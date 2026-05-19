use tempfile::tempdir;

#[test]
fn record_syntext_gitignore_sets_owned_line_flag() {
    let _lock = crate::test_support::global_test_lock(crate::config::test_home::HOME_ENV_TEST_LOCK);
    let home = tempdir().unwrap();
    let _guard = crate::config::test_home::HomeEnvGuard::redirect_to(home.path());
    let project = tempdir().unwrap();

    crate::registry::record_syntext_gitignore(project.path(), true).unwrap();
    crate::registry::record_syntext_gitignore(project.path(), false).unwrap();

    let entry = crate::registry::get(project.path()).unwrap().unwrap();
    assert!(entry.syntext_gitignore_entry_added);
    assert!(!entry.root_gitignore_entry_added);
}
