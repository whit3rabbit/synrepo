#[cfg(unix)]
use super::helpers::spawn_and_reap_pid;
use super::*;
use tempfile::tempdir;

#[test]
fn acquire_and_drop_removes_lock_file() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");

    let lock = acquire_writer_lock(&synrepo_dir).unwrap();
    assert!(lock.path().exists(), "lock file must exist while held");

    drop(lock);
    assert!(
        !writer_lock_path(&synrepo_dir).exists(),
        "lock file must be removed on drop"
    );
}

#[test]
fn lock_file_records_current_pid() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");

    let _lock = acquire_writer_lock(&synrepo_dir).unwrap();
    let owner = current_ownership(&synrepo_dir).expect("must read ownership");
    assert_eq!(owner.pid, std::process::id());
}

#[test]
fn current_ownership_is_none_after_lock_dropped() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");

    {
        let _lock = acquire_writer_lock(&synrepo_dir).unwrap();
    }
    assert_eq!(
        current_ownership(&synrepo_dir),
        Err(WriterOwnershipError::NotFound)
    );
}

#[test]
fn re_entrant_acquire_success() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");

    let _lock1 = acquire_writer_lock(&synrepo_dir).unwrap();
    let _lock2 = acquire_writer_lock(&synrepo_dir).expect("re-entrant acquire must succeed");

    assert!(writer_lock_path(&synrepo_dir).exists());

    drop(_lock2);
    assert!(
        writer_lock_path(&synrepo_dir).exists(),
        "lock file must still exist after inner drop"
    );

    drop(_lock1);
    assert!(
        !writer_lock_path(&synrepo_dir).exists(),
        "lock file must be removed after outer drop"
    );
}

#[cfg(unix)]
#[test]
fn stale_lock_from_dead_pid_is_replaced() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");
    std::fs::create_dir_all(synrepo_dir.join("state")).unwrap();

    let dead = spawn_and_reap_pid();
    let stale = WriterOwnership {
        pid: dead,
        acquired_at: "2000-01-01T00:00:00Z".to_string(),
    };
    std::fs::write(
        writer_lock_path(&synrepo_dir),
        serde_json::to_string_pretty(&stale).unwrap(),
    )
    .unwrap();

    let lock = acquire_writer_lock(&synrepo_dir).unwrap();
    let owner = current_ownership(&synrepo_dir).expect("must read ownership");
    assert_eq!(
        owner.pid,
        std::process::id(),
        "stale lock must be replaced with current PID"
    );
    drop(lock);
}

#[test]
fn malformed_lock_file_is_replaced() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");
    std::fs::create_dir_all(synrepo_dir.join("state")).unwrap();

    std::fs::write(writer_lock_path(&synrepo_dir), b"not valid json").unwrap();

    assert!(matches!(
        current_ownership(&synrepo_dir),
        Err(WriterOwnershipError::Malformed(_))
    ));

    let lock = acquire_writer_lock(&synrepo_dir).unwrap();
    assert!(lock.path().exists());
    drop(lock);
}

#[test]
fn concurrent_acquire_from_threads_is_rejected() {
    use std::sync::Arc;

    let dir = tempdir().unwrap();
    let synrepo_dir = Arc::new(dir.path().join(".synrepo"));

    let dir1 = Arc::clone(&synrepo_dir);
    let dir2 = Arc::clone(&synrepo_dir);

    let h1 = std::thread::spawn(move || acquire_writer_lock(&dir1));
    let h2 = std::thread::spawn(move || acquire_writer_lock(&dir2));

    let r1 = h1.join().unwrap();
    let r2 = h2.join().unwrap();

    // Exactly one thread must succeed and one must fail with WrongThread.
    let ok_count = [&r1, &r2].iter().filter(|r| r.is_ok()).count();
    assert_eq!(ok_count, 1, "exactly one thread should hold the lock");
    let wrong_thread_count = [&r1, &r2]
        .iter()
        .filter(|r| matches!(r, Err(LockError::WrongThread { .. })))
        .count();
    assert_eq!(
        wrong_thread_count, 1,
        "the other thread should get WrongThread"
    );
}

#[test]
fn lock_path_as_directory_surfaces_io_error() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");
    std::fs::create_dir_all(synrepo_dir.join("state")).unwrap();
    std::fs::create_dir_all(writer_lock_path(&synrepo_dir)).unwrap();

    match acquire_writer_lock(&synrepo_dir) {
        Err(LockError::Io { .. }) => {}
        other => panic!("expected Io error opening a directory as the lock file, got {other:?}"),
    }
}

#[cfg(unix)]
#[test]
fn second_open_of_same_lock_file_blocks() {
    use fs2::FileExt;

    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");
    let _lock = acquire_writer_lock(&synrepo_dir).unwrap();

    // The kernel flock lives on the sentinel file, not the metadata file.
    let sentinel = super::helpers::sentinel_path(&writer_lock_path(&synrepo_dir));
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&sentinel)
        .unwrap();
    match file.try_lock_exclusive() {
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
        other => {
            panic!("a second open of the held sentinel file should fail with WouldBlock, got {other:?}")
        }
    }
}

#[test]
fn drop_does_not_remove_replaced_lock_file() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");

    let lock = acquire_writer_lock(&synrepo_dir).unwrap();
    let replacement = WriterOwnership {
        pid: 777_777,
        acquired_at: "2099-01-01T00:00:00Z".to_string(),
    };
    std::fs::write(
        writer_lock_path(&synrepo_dir),
        serde_json::to_string(&replacement).unwrap(),
    )
    .unwrap();

    drop(lock);
    let owner = current_ownership(&synrepo_dir).expect("must read ownership");
    assert_eq!(
        owner, replacement,
        "drop must not delete a replacement lock"
    );
}

/// Write admission is rejected while a live watch owner holds the repo.
#[test]
fn write_admission_blocked_when_watch_running() {
    use crate::pipeline::watch::lease::acquire_watch_daemon_lease;
    use crate::pipeline::watch::WatchServiceMode;

    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");
    std::fs::create_dir_all(synrepo_dir.join("state")).unwrap();

    // Hold a real watch lease (kernel flock + state file) for the duration
    // of the admission check. Plain state-file seeding no longer signals a
    // live watch; the flock is the source of truth.
    let (_lease, _handle) =
        acquire_watch_daemon_lease(&synrepo_dir, WatchServiceMode::Daemon).unwrap();

    let result = acquire_write_admission(&synrepo_dir, "test_op");
    assert!(
        matches!(result, Err(LockError::WatchOwned { .. })),
        "expected WatchOwned error when watch is active, got {result:?}"
    );
}

/// Write admission succeeds after a stale watch state file is cleaned up.
#[cfg(unix)]
#[test]
fn write_admission_succeeds_after_stale_watch_cleanup() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");
    let state_dir = synrepo_dir.join("state");
    std::fs::create_dir_all(&state_dir).unwrap();

    // Write a watch-daemon.json with a dead PID.
    let dead_pid = spawn_and_reap_pid();
    let state_path = state_dir.join("watch-daemon.json");
    std::fs::write(
        &state_path,
        serde_json::to_string(&serde_json::json!({
            "pid": dead_pid,
            "started_at": "2026-01-01T00:00:00Z",
            "mode": "daemon",
            "socket_path": "/tmp/nonexistent.sock",
        }))
        .unwrap(),
    )
    .unwrap();

    // acquire_write_admission should clean up the stale state and succeed.
    let lock = acquire_write_admission(&synrepo_dir, "test_op").unwrap();
    assert!(lock.path().exists());
    drop(lock);
}
