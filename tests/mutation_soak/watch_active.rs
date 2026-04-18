use std::fs;

use super::support::{
    assert_failure_contains, compact_state_path, export_manifest_path, init_repo, read_optional,
    reconcile_state_path, run, start_watch_daemon, stop_watch, synrepo_dir, write_upgrade_drift,
};

#[test]
#[ignore = "release-gate soak test; run with `cargo test --test mutation_soak -- --ignored --test-threads=1`"]
fn mutation_commands_stay_blocked_while_watch_is_active() {
    let repo = init_repo();
    let repo_path = repo.path();
    let export_dir = repo_path.join("synrepo-context");
    fs::create_dir_all(&export_dir).expect("create export dir");
    let sentinel = export_dir.join("sentinel.txt");
    fs::write(&sentinel, "watch-active sentinel").expect("write sentinel");

    start_watch_daemon(repo_path);
    write_upgrade_drift(repo_path);

    let reconcile_before = read_optional(&reconcile_state_path(repo_path));
    let compact_state = compact_state_path(repo_path);
    let manifest = export_manifest_path(repo_path);

    for _ in 0..10 {
        assert_failure_contains(run(repo_path, &["export"]), "watch service is active");
        assert_eq!(
            fs::read_to_string(&sentinel).expect("read sentinel"),
            "watch-active sentinel"
        );
        assert!(
            !manifest.exists(),
            "blocked export must not write a manifest"
        );

        assert_failure_contains(
            run(repo_path, &["compact", "--apply"]),
            "watch service is active",
        );
        assert!(
            !compact_state.exists(),
            "blocked compaction must not write compact-state.json"
        );

        assert_failure_contains(
            run(repo_path, &["upgrade", "--apply"]),
            "watch service is active",
        );
        assert_eq!(
            read_optional(&reconcile_state_path(repo_path)),
            reconcile_before,
            "blocked upgrade must not advance reconcile state"
        );
    }

    stop_watch(repo_path);
    assert!(
        synrepo_dir(repo_path)
            .join("state/watch-daemon.json")
            .exists()
            || true
    );
}
