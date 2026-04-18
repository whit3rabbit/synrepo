use super::*;
use std::process::Command;
use tempfile::tempdir;

fn run(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .status()
        .unwrap();
    assert!(status.success(), "git {:?} failed", args);
}

fn init_repo_with_commit(dir: &Path, file: &str, body: &str) {
    run(dir, &["init", "-q", "-b", "main"]);
    run(dir, &["config", "user.name", "Test"]);
    run(dir, &["config", "user.email", "test@example.com"]);
    std::fs::write(dir.join(file), body).unwrap();
    run(dir, &["add", file]);
    run(dir, &["commit", "-q", "-m", "initial"]);
}

fn amend_file(dir: &Path, file: &str, body: &str, message: &str) {
    std::fs::write(dir.join(file), body).unwrap();
    run(dir, &["add", file]);
    run(dir, &["commit", "-q", "-m", message]);
}

#[test]
fn non_git_repo_latches_unavailable_once() {
    let tmp = tempdir().unwrap();
    let config = Config::default();
    let cache = GitCache::new();

    assert!(cache
        .resolve_path(tmp.path(), &config, "src/lib.rs")
        .is_none());
    assert!(matches!(&*cache.inner.read(), Inner::Unavailable));

    assert!(cache
        .resolve_path(tmp.path(), &config, "src/other.rs")
        .is_none());
    assert!(matches!(&*cache.inner.read(), Inner::Unavailable));
}

#[test]
fn head_change_rebuilds_index() {
    let tmp = tempdir().unwrap();
    let repo = tmp.path();
    let config = Config::default();
    init_repo_with_commit(repo, "file.txt", "one\n");

    let cache = GitCache::new();
    let first = cache.resolve_path(repo, &config, "file.txt").unwrap();
    assert_eq!(first.commits.len(), 1);
    let first_index = cache.index_ptr().unwrap();

    amend_file(repo, "file.txt", "two\n", "second");
    cache.force_head_probe();

    let second = cache.resolve_path(repo, &config, "file.txt").unwrap();
    assert_eq!(second.commits.len(), 2);
    let second_index = cache.index_ptr().unwrap();
    assert_ne!(
        first_index, second_index,
        "HEAD move must rebuild the index Arc"
    );
}

#[test]
fn head_unchanged_reuses_index() {
    let tmp = tempdir().unwrap();
    let repo = tmp.path();
    let config = Config::default();
    init_repo_with_commit(repo, "file.txt", "one\n");

    let cache = GitCache::new();
    cache.resolve_path(repo, &config, "file.txt").unwrap();
    let first_index = cache.index_ptr().unwrap();

    cache.force_head_probe();
    cache.resolve_path(repo, &config, "file.txt").unwrap();
    let second_index = cache.index_ptr().unwrap();
    assert_eq!(
        first_index, second_index,
        "HEAD unchanged must preserve the index Arc"
    );
}

#[test]
fn fifo_eviction_drops_oldest_path() {
    // Exercise eviction purely at the map level — no git walk needed.
    let mut paths = BoundedPathCache::new(2);
    paths.insert("a".into(), None);
    paths.insert("b".into(), None);
    paths.insert("c".into(), None);
    assert!(!paths.map.contains_key("a"));
    assert!(paths.map.contains_key("b"));
    assert!(paths.map.contains_key("c"));
    assert_eq!(paths.order.len(), 2);
}

#[test]
fn fifo_rewrite_does_not_grow_order_queue() {
    let mut paths = BoundedPathCache::new(4);
    paths.insert("a".into(), None);
    paths.insert("a".into(), None);
    paths.insert("a".into(), None);
    assert_eq!(paths.order.len(), 1);
    assert_eq!(paths.map.len(), 1);
}

fn delete_file(dir: &Path, file: &str, message: &str) {
    std::fs::remove_file(dir.join(file)).unwrap();
    run(dir, &["add", file]);
    run(dir, &["commit", "-q", "-m", message]);
}

#[test]
fn delete_and_recreate_invalidates_cache() {
    let tmp = tempdir().unwrap();
    let repo = tmp.path();
    let config = Config::default();

    // Commit file.txt with content "A"
    init_repo_with_commit(repo, "file.txt", "content A\n");

    let cache = GitCache::new();
    let first = cache.resolve_path(repo, &config, "file.txt").unwrap();
    assert_eq!(first.commits.len(), 1);

    // Delete and commit
    delete_file(repo, "file.txt", "delete");

    // Re-create with new content and commit
    amend_file(repo, "file.txt", "content B\n", "recreate");

    // Force probe to invalidate stale cache
    cache.on_compile_cycle_end();

    // Resolve again - should see new history
    let second = cache.resolve_path(repo, &config, "file.txt").unwrap();
    assert_eq!(
        second.commits.len(),
        3,
        "should see 3 commits: initial, delete, recreate"
    );
}
