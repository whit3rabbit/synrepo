#![cfg(unix)]

use std::{
    fs,
    time::{Duration, Instant},
};

use crate::{
    config::Config,
    pipeline::{
        export::{load_manifest, ExportFormat, ExportManifest, MANIFEST_FILENAME},
        repair::RepairSurface,
        watch::{
            lease::WatchStateHandle, request_watch_control, run_watch_service,
            watch_daemon_state_path, watch_service_status, SyncTrigger, WatchConfig,
            WatchControlRequest, WatchControlResponse, WatchDaemonState, WatchEvent,
            WatchServiceMode, WatchServiceStatus,
        },
    },
    store::compatibility::GRAPH_FORMAT_VERSION,
};

use super::{setup_test_repo, wait_for, watch_service_guard};

#[test]
fn watch_auto_sync_repairs_stale_export_after_startup_reconcile() {
    let _guard = watch_service_guard();
    let (_dir, repo, config, synrepo_dir) = setup_test_repo();
    write_stale_export_manifest(&repo, &config);

    let service_repo = repo.clone();
    let service_config = config.clone();
    let service_synrepo = synrepo_dir.clone();
    let (event_tx, event_rx) = crossbeam_channel::bounded::<WatchEvent>(32);

    let handle = std::thread::spawn(move || {
        run_watch_service(
            &service_repo,
            &service_config,
            &WatchConfig::default(),
            &service_synrepo,
            WatchServiceMode::Foreground,
            Some(event_tx),
        )
        .unwrap();
    });

    wait_for_service(&synrepo_dir);
    let summary = recv_auto_sync_finished(&event_rx);
    assert_summary_repairs_only_export(summary);

    let manifest = load_manifest(&repo, &config).expect("export manifest should exist");
    assert_ne!(manifest.last_reconcile_at, "stale-epoch");
    let state = request_watch_status(&synrepo_dir);
    assert!(state.auto_sync_enabled);
    assert!(!state.auto_sync_running);
    assert!(!state.auto_sync_paused);
    assert_eq!(state.auto_sync_last_outcome.as_deref(), Some("completed"));

    let _ = request_watch_control(&synrepo_dir, WatchControlRequest::Stop);
    handle.join().unwrap();
}

#[test]
fn watch_auto_sync_repairs_only_cheap_surfaces_after_manual_reconcile() {
    let _guard = watch_service_guard();
    let (_dir, repo, mut config, synrepo_dir) = setup_test_repo();
    config.auto_sync_enabled = false;

    let service_repo = repo.clone();
    let service_config = config.clone();
    let service_synrepo = synrepo_dir.clone();
    let (event_tx, event_rx) = crossbeam_channel::bounded::<WatchEvent>(32);

    let handle = std::thread::spawn(move || {
        run_watch_service(
            &service_repo,
            &service_config,
            &WatchConfig::default(),
            &service_synrepo,
            WatchServiceMode::Foreground,
            Some(event_tx),
        )
        .unwrap();
    });

    wait_for_service(&synrepo_dir);
    assert_no_auto_sync_event(&event_rx, Duration::from_millis(200));
    let response = request_watch_control(
        &synrepo_dir,
        WatchControlRequest::SetAutoSync { enabled: true },
    )
    .expect("enable auto-sync");
    assert!(
        matches!(response, WatchControlResponse::Ack { .. }),
        "expected auto-sync ack, got {response:?}"
    );
    write_stale_export_manifest(&repo, &config);
    request_reconcile(&synrepo_dir);
    let summary = recv_auto_sync_finished(&event_rx);
    assert_summary_repairs_only_export(summary);

    let manifest = load_manifest(&repo, &config).expect("export manifest should exist");
    assert_ne!(manifest.last_reconcile_at, "stale-epoch");
    let state = request_watch_status(&synrepo_dir);
    assert!(state.auto_sync_enabled);
    assert_eq!(state.auto_sync_last_outcome.as_deref(), Some("completed"));

    let _ = request_watch_control(&synrepo_dir, WatchControlRequest::Stop);
    handle.join().unwrap();
}

fn assert_summary_repairs_only_export(summary: crate::pipeline::repair::SyncSummary) {
    let repaired: Vec<_> = summary
        .repaired
        .iter()
        .map(|finding| finding.surface)
        .collect();
    assert!(
        repaired.contains(&RepairSurface::ExportSurface),
        "stale export should be repaired by auto-sync; repaired={repaired:?}"
    );
    assert!(
        repaired
            .iter()
            .all(|surface| crate::pipeline::repair::CHEAP_AUTO_SYNC_SURFACES.contains(surface)),
        "auto-sync must only repair cheap surfaces; repaired={repaired:?}"
    );
}

#[test]
fn watch_auto_sync_disabled_skips() {
    let _guard = watch_service_guard();
    let (_dir, repo, mut config, synrepo_dir) = setup_test_repo();
    config.auto_sync_enabled = false;
    write_stale_export_manifest(&repo, &config);

    let service_repo = repo.clone();
    let service_config = config.clone();
    let service_synrepo = synrepo_dir.clone();
    let (event_tx, event_rx) = crossbeam_channel::bounded::<WatchEvent>(32);

    let handle = std::thread::spawn(move || {
        run_watch_service(
            &service_repo,
            &service_config,
            &WatchConfig::default(),
            &service_synrepo,
            WatchServiceMode::Foreground,
            Some(event_tx),
        )
        .unwrap();
    });

    wait_for_service(&synrepo_dir);
    request_reconcile(&synrepo_dir);
    assert_no_auto_sync_event(&event_rx, Duration::from_millis(500));
    let manifest = load_manifest(&repo, &config).expect("export manifest should exist");
    assert_eq!(manifest.last_reconcile_at, "stale-epoch");
    assert!(!request_watch_status(&synrepo_dir).auto_sync_enabled);

    let _ = request_watch_control(&synrepo_dir, WatchControlRequest::Stop);
    handle.join().unwrap();
}

#[test]
fn watch_state_tracks_blocked_auto_sync_and_manual_recovery() {
    let (_dir, _repo, _config, synrepo_dir) = setup_test_repo();
    let state_path = watch_daemon_state_path(&synrepo_dir);
    let handle = WatchStateHandle::new(
        state_path,
        WatchDaemonState::new(&synrepo_dir, WatchServiceMode::Foreground),
    );

    handle.note_auto_sync_enabled(true);
    handle.note_auto_sync_started();
    let running = handle.snapshot();
    assert!(running.auto_sync_enabled);
    assert!(running.auto_sync_running);
    assert!(!running.auto_sync_paused);
    assert_eq!(running.auto_sync_last_outcome.as_deref(), Some("running"));

    handle.note_auto_sync_finished(true);
    let paused = handle.snapshot();
    assert!(!paused.auto_sync_running);
    assert!(paused.auto_sync_paused);
    assert_eq!(paused.auto_sync_last_outcome.as_deref(), Some("blocked"));

    handle.note_manual_sync_finished(false);
    let recovered = handle.snapshot();
    assert!(!recovered.auto_sync_paused);
    assert_eq!(
        recovered.auto_sync_last_outcome.as_deref(),
        Some("manual_sync_completed")
    );
}

fn wait_for_service(synrepo_dir: &std::path::Path) {
    wait_for(
        || {
            matches!(
                watch_service_status(synrepo_dir),
                WatchServiceStatus::Running(_)
            ) && super::super::watch_socket_path(synrepo_dir).exists()
        },
        Duration::from_secs(5),
    );
}

fn request_reconcile(synrepo_dir: &std::path::Path) {
    let response = request_watch_control(
        synrepo_dir,
        WatchControlRequest::ReconcileNow { fast: false },
    )
    .expect("request reconcile");
    assert!(
        matches!(response, WatchControlResponse::Reconcile { .. }),
        "expected reconcile response, got {response:?}"
    );
}

fn request_watch_status(synrepo_dir: &std::path::Path) -> WatchDaemonState {
    match request_watch_control(synrepo_dir, WatchControlRequest::Status).expect("watch status") {
        WatchControlResponse::Status { snapshot } => snapshot,
        other => panic!("expected status response, got {other:?}"),
    }
}

fn write_stale_export_manifest(repo: &std::path::Path, config: &Config) {
    let export_dir = repo.join(&config.export_dir);
    fs::create_dir_all(&export_dir).expect("create export dir");
    let manifest = ExportManifest {
        graph_schema_version: GRAPH_FORMAT_VERSION,
        last_reconcile_at: "stale-epoch".to_string(),
        format: ExportFormat::Markdown,
        budget: "normal".to_string(),
        generated_at: "2026-01-01T00:00:00Z".to_string(),
    };
    fs::write(
        export_dir.join(MANIFEST_FILENAME),
        serde_json::to_string_pretty(&manifest).expect("serialize manifest"),
    )
    .expect("write export manifest");
}

fn recv_auto_sync_finished(
    rx: &crossbeam_channel::Receiver<WatchEvent>,
) -> crate::pipeline::repair::SyncSummary {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if let Ok(event) = rx.recv_timeout(Duration::from_millis(100)) {
            if let WatchEvent::SyncFinished {
                trigger: SyncTrigger::AutoPostReconcile,
                summary,
                ..
            } = event
            {
                return summary;
            }
        }
    }
    panic!("timed out waiting for auto-sync finish event");
}

fn assert_no_auto_sync_event(rx: &crossbeam_channel::Receiver<WatchEvent>, duration: Duration) {
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        if let Ok(event) = rx.recv_timeout(Duration::from_millis(50)) {
            match event {
                WatchEvent::SyncStarted {
                    trigger: SyncTrigger::AutoPostReconcile,
                    ..
                }
                | WatchEvent::SyncFinished {
                    trigger: SyncTrigger::AutoPostReconcile,
                    ..
                } => panic!("auto-sync should be disabled, got {event:?}"),
                _ => {}
            }
        }
    }
}
