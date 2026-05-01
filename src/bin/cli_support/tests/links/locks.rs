#![cfg(unix)]

use synrepo::config::Config;
use synrepo::core::ids::NodeId;
use synrepo::overlay::{OverlayEdgeKind, OverlayStore};
use synrepo::store::overlay::{format_candidate_id, SqliteOverlayStore};
use tempfile::tempdir;

use super::support::seed_graph;
use super::{commands, sample_link, setup_curated_link_env, write_curated_mode};

#[cfg(unix)]
#[test]
fn links_accept_blocked_when_watch_running() {
    use synrepo::pipeline::watch::{
        hold_watch_flock_with_state, WatchDaemonState, WatchServiceMode,
    };

    let repo = tempdir().unwrap();
    let ids = seed_graph(repo.path());
    write_curated_mode(repo.path());

    let synrepo_dir = Config::synrepo_dir(repo.path());
    let mut overlay = SqliteOverlayStore::open(&synrepo_dir.join("overlay")).unwrap();
    let from = NodeId::Concept(ids.concept_id);
    let to = NodeId::Symbol(ids.symbol_id);
    overlay.insert_link(sample_link(from, to)).unwrap();

    // Hold the kernel flock on the watch sentinel and write a matching state
    // file so ensure_watch_not_running sees Running rather than Stale.
    let state = WatchDaemonState {
        pid: std::process::id(),
        started_at: "2026-04-15T00:00:00Z".to_string(),
        mode: WatchServiceMode::Foreground,
        control_endpoint: synrepo_dir.join("state/watch.sock").display().to_string(),
        last_event_at: None,
        last_reconcile_at: None,
        last_reconcile_outcome: None,
        last_error: None,
        last_triggering_events: None,
        auto_sync_enabled: false,
        auto_sync_running: false,
        auto_sync_paused: false,
        auto_sync_last_started_at: None,
        auto_sync_last_finished_at: None,
        auto_sync_last_outcome: None,
    };
    let _watch = hold_watch_flock_with_state(&synrepo_dir, &state);

    let candidate_id = format_candidate_id(from, to, OverlayEdgeKind::References, "test-pass");
    let err = commands::links_accept(repo.path(), &candidate_id, None).unwrap_err();
    assert!(
        err.to_string()
            .contains("links accept: watch service is active"),
        "expected watch-service guard error, got: {err}"
    );

    let reject_err = commands::links_reject(repo.path(), &candidate_id, None).unwrap_err();
    assert!(
        reject_err
            .to_string()
            .contains("links reject: watch service is active"),
        "expected watch-service guard error from reject, got: {reject_err}"
    );
}

#[cfg(unix)]
#[test]
fn links_accept_fails_on_lock_conflict() {
    use synrepo::pipeline::writer::{
        hold_writer_flock_with_ownership, writer_lock_path, WriterOwnership,
    };

    let (repo, mut overlay, from, to) = setup_curated_link_env();
    overlay.insert_link(sample_link(from, to)).unwrap();

    let synrepo_dir = Config::synrepo_dir(repo.path());
    std::fs::create_dir_all(synrepo_dir.join("state")).unwrap();
    let mut holder = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .expect("spawn sleep");
    let ownership = WriterOwnership {
        pid: holder.id(),
        acquired_at: "2099-01-01T00:00:00Z".to_string(),
    };
    let _flock = hold_writer_flock_with_ownership(&writer_lock_path(&synrepo_dir), &ownership);

    let candidate_id = format_candidate_id(from, to, OverlayEdgeKind::References, "test-pass");
    let err = commands::links_accept(repo.path(), &candidate_id, Some("reviewer-a")).unwrap_err();
    let _ = holder.kill();
    let _ = holder.wait();

    assert!(
        err.to_string().contains("writer lock held by pid"),
        "expected lock conflict error, got: {err}"
    );
}

#[cfg(unix)]
#[test]
fn links_reject_fails_on_lock_conflict() {
    use synrepo::pipeline::writer::{
        hold_writer_flock_with_ownership, writer_lock_path, WriterOwnership,
    };

    let (repo, mut overlay, from, to) = setup_curated_link_env();
    overlay.insert_link(sample_link(from, to)).unwrap();

    let synrepo_dir = Config::synrepo_dir(repo.path());
    std::fs::create_dir_all(synrepo_dir.join("state")).unwrap();
    let mut holder = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .expect("spawn sleep");
    let ownership = WriterOwnership {
        pid: holder.id(),
        acquired_at: "2099-01-01T00:00:00Z".to_string(),
    };
    let _flock = hold_writer_flock_with_ownership(&writer_lock_path(&synrepo_dir), &ownership);

    let candidate_id = format_candidate_id(from, to, OverlayEdgeKind::References, "test-pass");
    let err = commands::links_reject(repo.path(), &candidate_id, Some("reviewer-b")).unwrap_err();
    let _ = holder.kill();
    let _ = holder.wait();

    assert!(
        err.to_string().contains("writer lock held by pid"),
        "expected lock conflict error, got: {err}"
    );
}
