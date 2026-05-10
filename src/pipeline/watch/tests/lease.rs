//! Durability tests for the watch-state persistence path.
//!
//! These cover the atomic-write contract: a failed write must not clobber the
//! previous coherent state, and every successful write must be fully readable.

use std::fs;

use super::super::lease::persist_watch_state_at;
use super::super::{load_watch_state, watch_daemon_state_path, WatchDaemonState, WatchServiceMode};
use super::setup_test_repo;

fn synthetic_state(pid: u32, started_at: &str, endpoint: &str) -> WatchDaemonState {
    let mut state = WatchDaemonState::new(std::path::Path::new("."), WatchServiceMode::Foreground);
    state.pid = pid;
    state.started_at = started_at.to_string();
    state.control_endpoint = endpoint.to_string();
    state
}

#[test]
fn persist_then_load_round_trips() {
    let (_dir, _repo, _config, synrepo_dir) = setup_test_repo();
    let state_path = watch_daemon_state_path(&synrepo_dir);
    let state = synthetic_state(42, "2026-04-18T00:00:00Z", "/tmp/fake.sock");

    persist_watch_state_at(&state_path, &state).unwrap();
    let loaded = load_watch_state(&synrepo_dir).unwrap();

    assert_eq!(loaded, state);
}

#[test]
fn overwriting_leaves_no_intermediate_mixed_state() {
    let (_dir, _repo, _config, synrepo_dir) = setup_test_repo();
    let state_path = watch_daemon_state_path(&synrepo_dir);

    let first = synthetic_state(11, "2026-04-18T00:00:00Z", "/tmp/first.sock");
    persist_watch_state_at(&state_path, &first).unwrap();

    let second = synthetic_state(22, "2026-04-18T00:00:01Z", "/tmp/second.sock");
    persist_watch_state_at(&state_path, &second).unwrap();

    // After the second write the target must parse as the second state
    // exactly, never as a partial blend of the two.
    let loaded = load_watch_state(&synrepo_dir).unwrap();
    assert_eq!(loaded, second);

    // No orphaned tmp files in the state directory.
    let leftover: Vec<_> = fs::read_dir(synrepo_dir.join("state"))
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains(".tmp."))
        .collect();
    assert!(
        leftover.is_empty(),
        "found leftover tmp files: {leftover:?}"
    );
}

#[test]
fn failed_write_preserves_prior_good_state() {
    let (_dir, _repo, _config, synrepo_dir) = setup_test_repo();
    let state_path = watch_daemon_state_path(&synrepo_dir);

    let good = synthetic_state(77, "2026-04-18T00:00:00Z", "/tmp/good.sock");
    persist_watch_state_at(&state_path, &good).unwrap();

    // Attempt to persist into a bogus nested path under a file (not a
    // directory) -- the tmp-file creation must fail and the existing good
    // state must remain intact.
    let block_path = synrepo_dir.join("state").join("not-a-dir");
    fs::write(&block_path, b"sentinel").unwrap();
    let bogus_target = block_path.join("watch-daemon.json");
    let bogus_state = synthetic_state(88, "2026-04-18T00:00:01Z", "/tmp/bad.sock");

    let err = persist_watch_state_at(&bogus_target, &bogus_state);
    assert!(
        err.is_err(),
        "expected failure when parent is not a directory"
    );

    // Prior state unchanged at its canonical path.
    let loaded = load_watch_state(&synrepo_dir).unwrap();
    assert_eq!(loaded, good);
}
