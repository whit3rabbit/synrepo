use std::{thread, time::Duration};

use serde_json::json;

use crate::{
    config::Config,
    pipeline::{
        watch::{
            control_endpoint_reachable, request_watch_control, run_watch_service,
            watch_service_status, WatchConfig, WatchControlRequest, WatchEvent, WatchServiceMode,
            WatchServiceStatus,
        },
        writer::acquire_writer_lock,
    },
};

use super::{apply, prepare, state_with_files};

#[test]
fn watch_active_apply_delegates_reconcile() {
    // Serialize with watch-service and HOME-mutating tests; both affect the
    // process-global watch control socket location.
    let _watch_lock = crate::test_support::global_test_lock("watch-service");
    let _home_flock =
        crate::test_support::global_test_lock(crate::config::test_home::HOME_ENV_TEST_LOCK);
    let _home_mutex = crate::config::test_home::lock_home_env_read();
    let (dir, state) = state_with_files(&[("src/lib.rs", "one\ntwo\n")]);
    let synrepo_dir = Config::synrepo_dir(dir.path());
    let repo_root = dir.path().to_path_buf();
    let mut config = Config::load(dir.path()).unwrap();
    config.auto_sync_enabled = false;

    let watch_synrepo_dir = synrepo_dir.clone();
    let watch_repo_root = repo_root.clone();
    let watch_config = config.clone();
    let (event_tx, event_rx) = crossbeam_channel::bounded::<WatchEvent>(32);
    let handle = thread::spawn(move || {
        run_watch_service(
            &watch_repo_root,
            &watch_config,
            &WatchConfig::default(),
            &watch_synrepo_dir,
            WatchServiceMode::Foreground,
            Some(event_tx),
        )
    });
    wait_for_watch(&synrepo_dir);
    wait_for_writer_lock_free(&synrepo_dir);
    drain_watch_events(&event_rx);

    let prepared = prepare(
        &state,
        json!({ "target": "src/lib.rs", "target_kind": "file", "task_id": "task-watch" }),
    );
    let result = apply(
        &state,
        json!({ "edits": [{
            "task_id": "task-watch",
            "anchor_state_version": prepared["anchor_state_version"],
            "path": "src/lib.rs",
            "content_hash": prepared["content_hash"],
            "anchor": "L000002",
            "edit_type": "replace",
            "text": "TWO"
        }] }),
    );

    assert_eq!(result["status"], "completed", "{result}");
    assert_eq!(result["diagnostics"]["reconcile"]["status"], "delegated");
    thread::sleep(Duration::from_millis(800));
    let events = drain_watch_events(&event_rx);
    assert!(
        events.iter().any(|event| matches!(
            event,
            WatchEvent::ReconcileFinished {
                triggering_events: 0,
                ..
            }
        )),
        "MCP apply should still delegate one explicit reconcile: {events:?}"
    );
    assert!(
        events.iter().all(|event| !matches!(
            event,
            WatchEvent::ReconcileFinished {
                triggering_events,
                ..
            } if *triggering_events > 0
        )),
        "suppressed edit writes should not trigger a watcher reconcile: {events:?}"
    );

    let _ = request_watch_control(&synrepo_dir, WatchControlRequest::Stop);
    let joined = handle.join().expect("watch thread should not panic");
    assert!(
        joined.is_ok(),
        "watch service should stop cleanly: {joined:?}"
    );
}

fn wait_for_watch(synrepo_dir: &std::path::Path) {
    for _ in 0..100 {
        if matches!(
            watch_service_status(synrepo_dir),
            WatchServiceStatus::Running(_)
        ) && control_endpoint_reachable(synrepo_dir)
        {
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }
    panic!("watch service did not become ready");
}

fn wait_for_writer_lock_free(synrepo_dir: &std::path::Path) {
    for _ in 0..200 {
        if let Ok(lock) = acquire_writer_lock(synrepo_dir) {
            drop(lock);
            return;
        }
        thread::sleep(Duration::from_millis(25));
    }
    panic!("writer lock did not become free");
}

fn drain_watch_events(rx: &crossbeam_channel::Receiver<WatchEvent>) -> Vec<WatchEvent> {
    let mut events = Vec::new();
    while let Ok(event) = rx.try_recv() {
        events.push(event);
    }
    events
}
