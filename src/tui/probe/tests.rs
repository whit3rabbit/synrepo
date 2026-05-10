//! Unit tests for probe view models.

use std::{collections::BTreeMap, path::PathBuf, time::Duration};

use crate::bootstrap::runtime_probe::{AgentIntegration, AgentTargetKind};
use crate::pipeline::diagnostics::{
    EmbeddingHealth, ReconcileHealth, ReconcileStaleness, RuntimeDiagnostics, WriterStatus,
};
use crate::pipeline::watch::{WatchDaemonState, WatchServiceMode, WatchServiceStatus};
use crate::store::sqlite::PersistedGraphStats;
use crate::surface::status_snapshot::{
    CommentaryCoverage, ExportState, ExportStatus, GraphSnapshotStatus, OverlayState,
    RepairAuditState, StatusSnapshot,
};

use super::{build_health_vm, build_next_actions_with_context, NextActionRuntime, Severity};

fn snapshot() -> StatusSnapshot {
    StatusSnapshot {
        initialized: true,
        config: None,
        diagnostics: None,
        graph_stats: None,
        graph_snapshot: GraphSnapshotStatus {
            epoch: 0,
            age_ms: 0,
            size_bytes: 0,
            file_count: 0,
            symbol_count: 0,
            edge_count: 0,
        },
        export_freshness: "current".to_string(),
        export_status: ExportStatus {
            state: ExportState::Current,
            display: "current".to_string(),
            export_dir: "synrepo-context".to_string(),
            format: Some("markdown".to_string()),
            budget: Some("normal".to_string()),
        },
        overlay_cost_summary: "0".to_string(),
        overlay_state: OverlayState::Error,
        commentary_coverage: CommentaryCoverage {
            total: None,
            fresh: None,
            estimated_fresh: None,
            estimated_stale_ratio: None,
            estimate_confidence: None,
            display: "unavailable (test fixture)".to_string(),
        },
        agent_note_counts: None,
        explain_provider: None,
        explain_totals: None,
        context_metrics: None,
        last_compaction: None,
        repair_audit: RepairAuditState::Ok,
        recent_activity: None,
        synrepo_dir: PathBuf::from("/tmp/probe-test"),
    }
}

fn snapshot_for_actions(
    export_freshness: &str,
    reconcile_health: ReconcileHealth,
    watch_status: WatchServiceStatus,
    writer_status: WriterStatus,
) -> StatusSnapshot {
    let mut snapshot = snapshot();
    snapshot.graph_stats = Some(PersistedGraphStats {
        file_nodes: 1,
        symbol_nodes: 1,
        concept_nodes: 0,
        total_edges: 0,
        edge_counts_by_kind: BTreeMap::new(),
    });
    snapshot.export_freshness = export_freshness.to_string();
    snapshot.export_status = export_status_from_display(export_freshness);
    snapshot.diagnostics = Some(RuntimeDiagnostics {
        reconcile_health,
        watch_status,
        writer_status,
        store_guidance: Vec::new(),
        last_reconcile: None,
        embedding_health: EmbeddingHealth::Disabled,
    });
    snapshot
}

fn snapshot_for_store_guidance(
    guidance: Vec<String>,
    watch_status: WatchServiceStatus,
) -> StatusSnapshot {
    let mut snapshot = snapshot_for_actions(
        "current",
        ReconcileHealth::Current,
        watch_status,
        WriterStatus::Free,
    );
    snapshot
        .diagnostics
        .as_mut()
        .expect("fixture has diagnostics")
        .store_guidance = guidance;
    snapshot
}

fn export_status_from_display(display: &str) -> ExportStatus {
    let state = if display.starts_with("stale") {
        ExportState::Stale
    } else if display.starts_with("not generated") {
        ExportState::Absent
    } else {
        ExportState::Current
    };
    ExportStatus {
        state,
        display: display.to_string(),
        export_dir: "synrepo-context".to_string(),
        format: Some("markdown".to_string()).filter(|_| state != ExportState::Absent),
        budget: Some("normal".to_string()).filter(|_| state != ExportState::Absent),
    }
}

fn watch_state() -> WatchDaemonState {
    let mut state =
        WatchDaemonState::new(std::path::Path::new(".synrepo"), WatchServiceMode::Daemon);
    state.pid = 42;
    state.started_at = "2026-05-01T00:00:00Z".to_string();
    state.control_endpoint = "/tmp/synrepo-test.sock".to_string();
    state.auto_sync_enabled = true;
    state
}

fn complete_integration() -> AgentIntegration {
    AgentIntegration::Complete {
        target: AgentTargetKind::Codex,
    }
}

fn runtime(due_in: Duration) -> NextActionRuntime<'static> {
    NextActionRuntime {
        snapshot_refresh_due_in: due_in,
        auto_sync_enabled: None,
        materialize_state: None,
        now: time::OffsetDateTime::parse(
            "2026-05-01T00:00:05Z",
            &time::format_description::well_known::Rfc3339,
        )
        .unwrap(),
    }
}

#[test]
fn export_stale_with_watch_auto_sync_shows_automatic_wait() {
    let snapshot = snapshot_for_actions(
        "stale (generated at old, current epoch new)",
        ReconcileHealth::Current,
        WatchServiceStatus::Running(watch_state()),
        WriterStatus::Free,
    );

    let actions = build_next_actions_with_context(
        &snapshot,
        &complete_integration(),
        runtime(Duration::from_millis(1500)),
    );
    let labels: Vec<_> = actions.iter().map(|a| a.label.as_str()).collect();

    assert!(labels.contains(&"Context export refresh is automatic, checking again in 2s"));
    assert!(!labels.iter().any(|label| label.contains("synrepo export")));
}

#[test]
fn export_stale_with_running_auto_sync_shows_running_timer() {
    let mut state = watch_state();
    state.auto_sync_running = true;
    state.auto_sync_last_started_at = Some("2026-05-01T00:00:00Z".to_string());
    let snapshot = snapshot_for_actions(
        "stale (generated at old, current epoch new)",
        ReconcileHealth::Current,
        WatchServiceStatus::Running(state),
        WriterStatus::Free,
    );

    let actions = build_next_actions_with_context(
        &snapshot,
        &complete_integration(),
        runtime(Duration::from_secs(2)),
    );

    assert!(actions
        .iter()
        .any(|a| a.label == "Context export refresh running, started 5s ago"));
}

#[test]
fn export_stale_with_paused_auto_sync_points_to_sync() {
    let mut state = watch_state();
    state.auto_sync_paused = true;
    let snapshot = snapshot_for_actions(
        "stale (generated at old, current epoch new)",
        ReconcileHealth::Current,
        WatchServiceStatus::Running(state),
        WriterStatus::Free,
    );

    let actions = build_next_actions_with_context(
        &snapshot,
        &complete_integration(),
        runtime(Duration::from_secs(2)),
    );

    assert!(actions
        .iter()
        .any(|a| a.label == "Auto-sync paused after blocked repair, press S to inspect"));
}

#[test]
fn export_stale_without_auto_sync_uses_manual_sync_hint() {
    let inactive = snapshot_for_actions(
        "stale (generated at old, current epoch new)",
        ReconcileHealth::Current,
        WatchServiceStatus::Inactive,
        WriterStatus::Free,
    );
    let mut disabled_state = watch_state();
    disabled_state.auto_sync_enabled = false;
    let disabled = snapshot_for_actions(
        "stale (generated at old, current epoch new)",
        ReconcileHealth::Current,
        WatchServiceStatus::Running(disabled_state),
        WriterStatus::Free,
    );

    for snapshot in [inactive, disabled] {
        let actions = build_next_actions_with_context(
            &snapshot,
            &complete_integration(),
            runtime(Duration::ZERO),
        );
        assert!(actions
            .iter()
            .any(|a| a.label == "Context export stale, press S to refresh"));
    }
}

#[test]
fn absent_context_export_is_healthy_and_has_no_next_action() {
    let snapshot = snapshot_for_actions(
        "not generated (optional; synrepo export writes synrepo-context/)",
        ReconcileHealth::Current,
        WatchServiceStatus::Inactive,
        WriterStatus::Free,
    );

    let health = build_health_vm(&snapshot);
    let export_row = health
        .rows
        .iter()
        .find(|row| row.label == "context export")
        .expect("context export row must be present");
    assert_eq!(export_row.severity, Severity::Healthy);

    let actions = build_next_actions_with_context(
        &snapshot,
        &complete_integration(),
        runtime(Duration::ZERO),
    );
    assert!(!actions
        .iter()
        .any(|action| action.label.contains("Context export")));
}

#[test]
fn stale_reconcile_with_active_watch_waits_for_poll() {
    let snapshot = snapshot_for_actions(
        "current",
        ReconcileHealth::Stale(ReconcileStaleness::Outcome("lock-conflict".to_string())),
        WatchServiceStatus::Running(watch_state()),
        WriterStatus::HeldByOther { pid: 99 },
    );

    let actions = build_next_actions_with_context(
        &snapshot,
        &complete_integration(),
        runtime(Duration::from_secs(2)),
    );

    assert!(actions.iter().any(|a| {
        a.label == "Watch reconcile waiting on writer lock held by pid 99, checking again in 2s"
    }));
}

#[test]
fn compatibility_rebuild_next_action_uses_u_when_watch_inactive() {
    let snapshot = snapshot_for_store_guidance(
        vec![
            "index needs rebuild because index-sensitive config changed; run `synrepo upgrade --apply`"
                .to_string(),
        ],
        WatchServiceStatus::Inactive,
    );

    let actions = build_next_actions_with_context(
        &snapshot,
        &complete_integration(),
        runtime(Duration::ZERO),
    );

    assert!(actions
        .iter()
        .any(|a| a.label == "Compatibility rebuild pending, press U to apply"));
    assert!(!actions.iter().any(|a| a.label.contains("press B")));
}

#[test]
fn compatibility_rebuild_next_action_requires_stopping_watch() {
    let snapshot = snapshot_for_store_guidance(
        vec![
            "index needs rebuild because index-sensitive config changed; run `synrepo upgrade --apply`"
                .to_string(),
        ],
        WatchServiceStatus::Running(watch_state()),
    );

    let actions = build_next_actions_with_context(
        &snapshot,
        &complete_integration(),
        runtime(Duration::from_secs(2)),
    );

    assert!(actions.iter().any(|a| {
        a.label == "Compatibility rebuild pending, stop watch before pressing U to apply"
    }));
    assert!(!actions.iter().any(|a| a.label.contains("press B")));
}
