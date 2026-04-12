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
    let owner = current_ownership(&synrepo_dir).unwrap();
    assert_eq!(owner.pid, std::process::id());
}

#[test]
fn current_ownership_is_none_after_lock_dropped() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");

    {
        let _lock = acquire_writer_lock(&synrepo_dir).unwrap();
    }
    assert!(current_ownership(&synrepo_dir).is_none());
}

#[test]
fn second_acquire_fails_while_first_lock_is_live() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");

    let _lock = acquire_writer_lock(&synrepo_dir).unwrap();
    let result = acquire_writer_lock(&synrepo_dir);
    assert!(
        matches!(result, Err(LockError::HeldByOther { .. })),
        "expected HeldByOther, got {result:?}",
    );
}

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
    let owner = current_ownership(&synrepo_dir).unwrap();
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

    let lock = acquire_writer_lock(&synrepo_dir).unwrap();
    assert!(lock.path().exists());
    drop(lock);
}

#[test]
fn concurrent_acquire_only_one_succeeds() {
    use std::sync::Arc;

    let dir = tempdir().unwrap();
    let synrepo_dir = Arc::new(dir.path().join(".synrepo"));

    let dir1 = Arc::clone(&synrepo_dir);
    let dir2 = Arc::clone(&synrepo_dir);

    let h1 = std::thread::spawn(move || acquire_writer_lock(&dir1));
    let h2 = std::thread::spawn(move || acquire_writer_lock(&dir2));

    let r1 = h1.join().unwrap();
    let r2 = h2.join().unwrap();

    let successes = [&r1, &r2].iter().filter(|r| r.is_ok()).count();
    let conflicts = [&r1, &r2]
        .iter()
        .filter(|r| matches!(r, Err(LockError::HeldByOther { .. })))
        .count();

    assert_eq!(successes, 1, "exactly one thread must acquire the lock");
    assert_eq!(conflicts, 1, "exactly one thread must see HeldByOther");
}

#[test]
fn stale_lock_cleanup_reports_remove_failure() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");
    std::fs::create_dir_all(synrepo_dir.join("state")).unwrap();
    std::fs::create_dir_all(writer_lock_path(&synrepo_dir)).unwrap();

    let result = acquire_writer_lock(&synrepo_dir);
    match result {
        Err(LockError::Io { source, .. }) => {
            assert_ne!(
                source.kind(),
                std::io::ErrorKind::AlreadyExists,
                "cleanup failure should surface the real filesystem error"
            );
        }
        other => panic!("expected Io cleanup failure, got {other:?}"),
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

    let owner = current_ownership(&synrepo_dir).unwrap();
    assert_eq!(
        owner, replacement,
        "drop must not delete a replacement lock"
    );
}
