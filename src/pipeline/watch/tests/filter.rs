use notify_debouncer_full::{
    notify::{event::ModifyKind, Event, EventKind},
    DebouncedEvent,
};
use std::time::Instant;

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
        &synrepo_dir,
        &[export_dir],
    );
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].paths[0], repo.join("src/lib.rs"));
}

fn debounced_event(event: Event) -> DebouncedEvent {
    DebouncedEvent::new(event, Instant::now())
}
