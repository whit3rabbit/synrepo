use notify_debouncer_full::{
    notify::{
        event::{ModifyKind, RemoveKind},
        Event, EventKind,
    },
    DebouncedEvent,
};
use std::{fs, path::PathBuf, time::Instant};

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
        &repo,
        &synrepo_dir,
        &[],
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
        &repo,
        &synrepo_dir,
        &[export_dir],
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
        &repo,
        &synrepo_dir,
        &[],
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
        &repo,
        &synrepo_dir,
        &[],
    );

    assert!(paths.is_empty());
}

#[test]
fn collect_repo_paths_keeps_missing_removal_paths() {
    let (_dir, repo, _config, synrepo_dir) = setup_test_repo();
    let source_remove_event = debounced_event(
        Event::new(EventKind::Remove(RemoveKind::File)).add_path(PathBuf::from("src/old.rs")),
    );

    let paths =
        super::super::filter::collect_repo_paths(&[source_remove_event], &repo, &synrepo_dir, &[]);

    assert_eq!(paths, vec![repo.join("src/old.rs")]);
}

fn debounced_event(event: Event) -> DebouncedEvent {
    DebouncedEvent::new(event, Instant::now())
}
