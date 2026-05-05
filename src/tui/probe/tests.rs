//! Unit tests for probe view models.

use std::{collections::BTreeMap, path::PathBuf, time::Duration};

use crate::bootstrap::runtime_probe::{AgentIntegration, AgentTargetKind};
use crate::pipeline::context_metrics::ContextMetrics;
use crate::pipeline::diagnostics::{
    EmbeddingHealth, ReconcileHealth, ReconcileStaleness, RuntimeDiagnostics, WriterStatus,
};
use crate::pipeline::watch::{WatchDaemonState, WatchServiceMode, WatchServiceStatus};
use crate::store::sqlite::PersistedGraphStats;
use crate::surface::status_snapshot::{
    CommentaryCoverage, GraphSnapshotStatus, RepairAuditState, StatusSnapshot,
};

use super::{build_health_vm, build_next_actions_with_context, NextActionRuntime, Severity};

fn snapshot_with_metrics(metrics: Option<ContextMetrics>) -> StatusSnapshot {
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
        overlay_cost_summary: "0".to_string(),
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
        context_metrics: metrics,
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
    let mut snapshot = snapshot_with_metrics(None);
    snapshot.graph_stats = Some(PersistedGraphStats {
        file_nodes: 1,
        symbol_nodes: 1,
        concept_nodes: 0,
        total_edges: 0,
        edge_counts_by_kind: BTreeMap::new(),
    });
    snapshot.export_freshness = export_freshness.to_string();
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

fn watch_state() -> WatchDaemonState {
    WatchDaemonState {
        pid: 42,
        started_at: "2026-05-01T00:00:00Z".to_string(),
        mode: WatchServiceMode::Daemon,
        control_endpoint: "/tmp/synrepo-test.sock".to_string(),
        last_event_at: None,
        last_reconcile_at: None,
        last_reconcile_outcome: None,
        last_error: None,
        last_triggering_events: None,
        auto_sync_enabled: true,
        auto_sync_running: false,
        auto_sync_paused: false,
        auto_sync_last_started_at: None,
        auto_sync_last_finished_at: None,
        auto_sync_last_outcome: None,
    }
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
fn health_rows_omit_context_metrics_when_absent() {
    let snapshot = snapshot_with_metrics(None);
    let vm = build_health_vm(&snapshot);
    let labels: Vec<_> = vm.rows.iter().map(|r| r.label.as_str()).collect();
    assert!(!labels.contains(&"context"));
    assert!(!labels.contains(&"tokens avoided"));
    assert!(!labels.contains(&"stale responses"));
}

#[test]
fn health_rows_surface_tokens_avoided_and_stale_responses() {
    let mut metrics = ContextMetrics::default();
    metrics.cards_served_total = 4;
    metrics.card_tokens_total = 800;
    metrics.raw_file_tokens_total = 8_000;
    metrics.estimated_tokens_saved_total = 7_200;
    let snapshot = snapshot_with_metrics(Some(metrics));

    let vm = build_health_vm(&snapshot);
    let tokens_row = vm
        .rows
        .iter()
        .find(|r| r.label == "tokens avoided")
        .expect("tokens avoided row must be present when metrics exist");
    assert_eq!(tokens_row.value, "7200 est.");
    assert_eq!(tokens_row.severity, Severity::Healthy);

    let stale_row = vm
        .rows
        .iter()
        .find(|r| r.label == "stale responses")
        .expect("stale responses row must be present when metrics exist");
    assert_eq!(stale_row.value, "0");
    assert_eq!(
        stale_row.severity,
        Severity::Healthy,
        "zero stale responses must stay Healthy"
    );
}

#[test]
fn stale_responses_escalates_to_stale_when_nonzero() {
    let mut metrics = ContextMetrics::default();
    metrics.cards_served_total = 10;
    metrics.stale_responses_total = 3;
    let snapshot = snapshot_with_metrics(Some(metrics));

    let vm = build_health_vm(&snapshot);
    let stale_row = vm
        .rows
        .iter()
        .find(|r| r.label == "stale responses")
        .expect("stale responses row must be present when metrics exist");
    assert_eq!(stale_row.value, "3");
    assert_eq!(
        stale_row.severity,
        Severity::Stale,
        "non-zero stale responses must escalate severity"
    );
}

#[test]
fn health_rows_surface_mcp_request_metrics() {
    let mut metrics = ContextMetrics::default();
    metrics.mcp_requests_total = 3;
    metrics.mcp_resource_reads_total = 1;
    metrics
        .mcp_tool_errors_total
        .insert("synrepo_search".to_string(), 1);
    let snapshot = snapshot_with_metrics(Some(metrics));

    let vm = build_health_vm(&snapshot);
    let mcp_row = vm
        .rows
        .iter()
        .find(|r| r.label == "mcp")
        .expect("MCP row must be present when request metrics exist");
    assert_eq!(mcp_row.value, "3 req, 1 resource, 1 error");
    assert_eq!(mcp_row.severity, Severity::Stale);
}

#[test]
fn health_rows_surface_fast_path_and_anchor_metrics() {
    let metrics = ContextMetrics {
        route_classifications_total: 5,
        context_fast_path_signals_total: 4,
        deterministic_edit_candidates_total: 2,
        anchored_edit_accepted_total: 3,
        anchored_edit_rejected_total: 1,
        estimated_llm_calls_avoided_total: 4,
        ..ContextMetrics::default()
    };
    let snapshot = snapshot_with_metrics(Some(metrics));

    let vm = build_health_vm(&snapshot);
    let fast_path = vm
        .rows
        .iter()
        .find(|r| r.label == "fast path")
        .expect("fast path row must be present");
    assert_eq!(fast_path.value, "5 routes, 4 signals, 2 edit candidates");
    assert_eq!(fast_path.severity, Severity::Healthy);

    let anchors = vm
        .rows
        .iter()
        .find(|r| r.label == "anchored edits")
        .expect("anchored edits row must be present");
    assert_eq!(anchors.value, "3 accepted, 1 rejected");
    assert_eq!(anchors.severity, Severity::Stale);
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

    assert!(labels.contains(&"Export refresh is automatic, checking again in 2s"));
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
        .any(|a| a.label == "Export refresh running, started 5s ago"));
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
            .any(|a| a.label == "Export stale, press S to sync"));
    }
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
