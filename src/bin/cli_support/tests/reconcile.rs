use synrepo::bootstrap::bootstrap;
use synrepo::config::Config;
use tempfile::tempdir;

#[test]
fn reconcile_completes_on_initialized_repo() {
    let repo = tempdir().unwrap();
    std::fs::create_dir_all(repo.path().join("src")).unwrap();
    std::fs::write(repo.path().join("src/lib.rs"), "pub fn greet() {}\n").unwrap();
    bootstrap(repo.path(), None).unwrap();

    super::super::commands::reconcile(repo.path()).unwrap();

    let synrepo_dir = synrepo::config::Config::synrepo_dir(repo.path());
    let state = synrepo::pipeline::watch::load_reconcile_state(&synrepo_dir)
        .expect("reconcile state must be written after reconcile");
    assert_eq!(state.last_outcome, "completed");
    assert!(
        state.files_discovered.unwrap_or(0) >= 1,
        "reconcile must discover at least src/lib.rs"
    );
}

#[test]
fn reconcile_refreshes_the_search_index() {
    let repo = tempdir().unwrap();
    std::fs::create_dir_all(repo.path().join("src")).unwrap();
    std::fs::write(
        repo.path().join("src/lib.rs"),
        "pub fn greet() { println!(\"old token\"); }\n",
    )
    .unwrap();
    bootstrap(repo.path(), None).unwrap();

    std::fs::write(
        repo.path().join("src/lib.rs"),
        "pub fn greet() { println!(\"new token\"); }\n",
    )
    .unwrap();

    super::super::commands::reconcile(repo.path()).unwrap();

    let config = Config::load(repo.path()).unwrap();
    let old_matches = synrepo::substrate::search(&config, repo.path(), "old token").unwrap();
    let new_matches = synrepo::substrate::search(&config, repo.path(), "new token").unwrap();

    assert!(old_matches.is_empty(), "stale index entry must be removed");
    assert_eq!(new_matches.len(), 1, "updated file must be searchable");
}
