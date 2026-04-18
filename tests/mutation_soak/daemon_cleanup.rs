use std::time::Duration;

use super::support::{
    init_repo, kill_pid, read_watch_pid, run_ok, start_watch_daemon, stop_watch, wait_for,
    watch_status,
};

#[test]
#[ignore = "release-gate soak test; run with `cargo test --test mutation_soak -- --ignored --test-threads=1`"]
fn watch_daemon_recovers_cleanly_after_abrupt_exit_soak() {
    let repo = init_repo();
    let repo_path = repo.path();

    for _ in 0..10 {
        start_watch_daemon(repo_path);
        let pid = read_watch_pid(repo_path);
        kill_pid(pid);

        wait_for(
            || watch_status(repo_path).contains("state:        stale"),
            Duration::from_secs(5),
        );

        let restarted = run_ok(repo_path, &["watch", "--daemon"]);
        assert!(restarted.contains("Started watch service in daemon mode"));
        stop_watch(repo_path);
    }
}
