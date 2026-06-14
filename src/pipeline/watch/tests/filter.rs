use notify_debouncer_full::{
    notify::{
        event::{ModifyKind, RemoveKind},
        Event, EventKind,
    },
    DebouncedEvent,
};
use std::{fs, path::PathBuf, time::Instant};

use super::super::filter::WatchIgnoreSet;
use super::setup_test_repo;

#[test]
fn filter_repo_events_ignores_synrepo_only_bursts() {
    let (_dir, repo, _config, synrepo_dir) = setup_test_repo();
    let runtime_event = debounced_event(
        Event::new(EventKind::Modify(ModifyKind::Any))
            .add_path(synrepo_dir.join("state/watch-daemon.json"))
            .add_path(repo.clone()),
    );
    let source_event = debounced_event(
        Event::new(EventKind::Modify(ModifyKind::Any)).add_path(repo.join("src/lib.rs")),
    );

    let filtered = super::super::filter::filter_repo_events(
        vec![runtime_event, source_event],
        std::slice::from_ref(&repo),
        &repo,
        &synrepo_dir,
        &[],
        &repo_ignore_set(&repo),
    );
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].paths[0], repo.join("src/lib.rs"));
}

#[test]
fn filter_repo_events_ignores_generated_export_bursts() {
    let (_dir, repo, _config, synrepo_dir) = setup_test_repo();
    let export_dir = repo.join("synrepo-context");
    let export_event = debounced_event(
        Event::new(EventKind::Modify(ModifyKind::Any))
            .add_path(export_dir.join("files.md"))
            .add_path(export_dir.clone()),
    );
    let source_event = debounced_event(
        Event::new(EventKind::Modify(ModifyKind::Any)).add_path(repo.join("src/lib.rs")),
    );

    let filtered = super::super::filter::filter_repo_events(
        vec![export_event, source_event],
        std::slice::from_ref(&repo),
        &repo,
        &synrepo_dir,
        &[export_dir],
        &repo_ignore_set(&repo),
    );
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].paths[0], repo.join("src/lib.rs"));
}

#[test]
fn filter_repo_events_ignores_gitignored_target_bursts() {
    let (_dir, repo, _config, synrepo_dir) = setup_test_repo();
    fs::write(repo.join(".gitignore"), "target/\n").unwrap();
    fs::create_dir_all(repo.join("target/debug/deps")).unwrap();
    let target_file = repo.join("target/debug/deps/jsonrpc_tests-abc123");
    fs::write(&target_file, "test binary").unwrap();

    let ignore_set = repo_ignore_set(&repo);
    let target_event = debounced_event(
        Event::new(EventKind::Modify(ModifyKind::Any)).add_path(target_file.clone()),
    );
    let source_event = debounced_event(
        Event::new(EventKind::Modify(ModifyKind::Any)).add_path(repo.join("src/lib.rs")),
    );

    let filtered = super::super::filter::filter_repo_events(
        vec![target_event, source_event],
        std::slice::from_ref(&repo),
        &repo,
        &synrepo_dir,
        &[],
        &ignore_set,
    );

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].paths[0], repo.join("src/lib.rs"));

    let mixed_event = debounced_event(
        Event::new(EventKind::Modify(ModifyKind::Any))
            .add_path(target_file)
            .add_path(repo.join("src/lib.rs")),
    );
    let paths = super::super::filter::collect_repo_paths(
        &[mixed_event],
        std::slice::from_ref(&repo),
        &repo,
        &synrepo_dir,
        &[],
        &ignore_set,
    );

    assert_eq!(paths, vec![repo.join("src/lib.rs")]);
}

#[test]
fn filter_repo_events_ignores_syntext_only_bursts() {
    let (_dir, repo, _config, synrepo_dir) = setup_test_repo();
    let syntext_dir = repo.join(".syntext");
    fs::create_dir_all(&syntext_dir).unwrap();
    fs::write(syntext_dir.join("manifest.json"), "{}").unwrap();
    let runtime_event = debounced_event(
        Event::new(EventKind::Modify(ModifyKind::Any))
            .add_path(syntext_dir.join("manifest.json"))
            .add_path(syntext_dir.join("segment.post")),
    );
    let source_event = debounced_event(
        Event::new(EventKind::Modify(ModifyKind::Any)).add_path(repo.join("src/lib.rs")),
    );

    let filtered = super::super::filter::filter_repo_events(
        vec![runtime_event, source_event],
        std::slice::from_ref(&repo),
        &repo,
        &synrepo_dir,
        &[],
        &repo_ignore_set(&repo),
    );
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].paths[0], repo.join("src/lib.rs"));
}

#[test]
fn filter_repo_events_ignores_repo_relative_runtime_paths() {
    let (_dir, repo, _config, synrepo_dir) = setup_test_repo();
    fs::write(synrepo_dir.join("state/noise.txt"), "noise").unwrap();
    let runtime_event = debounced_event(
        Event::new(EventKind::Modify(ModifyKind::Any))
            .add_path(PathBuf::from(".synrepo/state/noise.txt"))
            .add_path(PathBuf::from("state/noise.txt")),
    );
    let source_event = debounced_event(
        Event::new(EventKind::Modify(ModifyKind::Any)).add_path(PathBuf::from("src/lib.rs")),
    );

    let filtered = super::super::filter::filter_repo_events(
        vec![runtime_event, source_event],
        std::slice::from_ref(&repo),
        &repo,
        &synrepo_dir,
        &[],
        &repo_ignore_set(&repo),
    );
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].paths[0], PathBuf::from("src/lib.rs"));
}

#[test]
fn filter_repo_events_ignores_repo_relative_syntext_paths() {
    let (_dir, repo, _config, synrepo_dir) = setup_test_repo();
    fs::create_dir_all(repo.join(".syntext")).unwrap();
    fs::write(repo.join(".syntext/manifest.json"), "{}").unwrap();
    let runtime_event = debounced_event(
        Event::new(EventKind::Modify(ModifyKind::Any))
            .add_path(PathBuf::from(".syntext/manifest.json")),
    );
    let source_event = debounced_event(
        Event::new(EventKind::Modify(ModifyKind::Any)).add_path(PathBuf::from("src/lib.rs")),
    );

    let filtered = super::super::filter::filter_repo_events(
        vec![runtime_event, source_event],
        std::slice::from_ref(&repo),
        &repo,
        &synrepo_dir,
        &[],
        &repo_ignore_set(&repo),
    );
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].paths[0], PathBuf::from("src/lib.rs"));
}

#[test]
fn collect_repo_paths_skips_missing_non_removal_paths() {
    let (_dir, repo, _config, synrepo_dir) = setup_test_repo();
    let ambiguous_runtime_event = debounced_event(
        Event::new(EventKind::Modify(ModifyKind::Any)).add_path(PathBuf::from("noise.txt")),
    );

    let paths = super::super::filter::collect_repo_paths(
        &[ambiguous_runtime_event],
        std::slice::from_ref(&repo),
        &repo,
        &synrepo_dir,
        &[],
        &repo_ignore_set(&repo),
    );

    assert!(paths.is_empty());
}

#[test]
fn collect_repo_paths_keeps_missing_removal_paths() {
    let (_dir, repo, _config, synrepo_dir) = setup_test_repo();
    let source_remove_event = debounced_event(
        Event::new(EventKind::Remove(RemoveKind::File)).add_path(PathBuf::from("src/old.rs")),
    );

    let paths = super::super::filter::collect_repo_paths(
        &[source_remove_event],
        std::slice::from_ref(&repo),
        &repo,
        &synrepo_dir,
        &[],
        &repo_ignore_set(&repo),
    );

    assert_eq!(paths, vec![repo.join("src/old.rs")]);
}

fn debounced_event(event: Event) -> DebouncedEvent {
    DebouncedEvent::new(event, Instant::now())
}

fn repo_ignore_set(repo: &PathBuf) -> WatchIgnoreSet {
    WatchIgnoreSet::from_roots(std::slice::from_ref(repo))
}
