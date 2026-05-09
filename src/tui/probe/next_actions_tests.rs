//! Focused next-action wording tests.

use std::{collections::BTreeMap, path::PathBuf, time::Duration};

use crate::bootstrap::runtime_probe::{AgentIntegration, AgentTargetKind};
use crate::config::Config;
use crate::pipeline::diagnostics::{
    EmbeddingHealth, ReconcileHealth, RuntimeDiagnostics, WriterStatus,
};
use crate::pipeline::watch::{WatchDaemonState, WatchServiceMode, WatchServiceStatus};
use crate::store::sqlite::PersistedGraphStats;
use crate::surface::status_snapshot::{
    CommentaryCoverage, ExportState, ExportStatus, GraphSnapshotStatus, OverlayState,
    RepairAuditState, StatusSnapshot,
};

use super::{build_next_actions_with_context, NextActionRuntime};

fn snapshot_for_store_guidance(watch_status: WatchServiceStatus, guidance: &str) -> StatusSnapshot {
    let mut config = Config::default();
    config.enable_semantic_triage = true;
    StatusSnapshot {
        initialized: true,
        config: Some(config),
        diagnostics: Some(RuntimeDiagnostics {
            reconcile_health: ReconcileHealth::Current,
            watch_status,
            writer_status: WriterStatus::Free,
            store_guidance: vec![guidance.to_string()],
            last_reconcile: None,
            embedding_health: EmbeddingHealth::Disabled,
        }),
        graph_stats: Some(PersistedGraphStats {
            file_nodes: 1,
            symbol_nodes: 1,
            concept_nodes: 0,
            total_edges: 0,
            edge_counts_by_kind: BTreeMap::new(),
        }),
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

fn complete_integration() -> AgentIntegration {
    AgentIntegration::Complete {
        target: AgentTargetKind::Codex,
    }
}

fn mcp_only_integration() -> AgentIntegration {
    AgentIntegration::McpOnly {
        target: AgentTargetKind::Codex,
    }
}

fn runtime() -> NextActionRuntime<'static> {
    NextActionRuntime {
        snapshot_refresh_due_in: Duration::ZERO,
        auto_sync_enabled: None,
        materialize_state: None,
        now: time::OffsetDateTime::UNIX_EPOCH,
    }
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

#[test]
fn compatibility_store_guidance_uses_u_when_watch_inactive() {
    let snapshot = snapshot_for_store_guidance(
        WatchServiceStatus::Inactive,
        "index needs rebuild because index-sensitive config changed (`roots`, `max_file_size_bytes`, or `redact_globs`); run `synrepo upgrade --apply`",
    );

    let actions = build_next_actions_with_context(&snapshot, &complete_integration(), runtime());

    assert!(actions
        .iter()
        .any(|a| a.label == "Compatibility rebuild pending, press U to apply"));
}

#[test]
fn compatibility_store_guidance_requires_stopping_watch() {
    let snapshot = snapshot_for_store_guidance(
        WatchServiceStatus::Running(watch_state()),
        "embeddings needs invalidation because index-sensitive config changed, so embeddings are stale; run `synrepo upgrade --apply`",
    );

    let actions = build_next_actions_with_context(&snapshot, &complete_integration(), runtime());

    assert!(actions.iter().any(|a| {
        a.label == "Compatibility action pending, stop watch before pressing U to apply"
    }));
}

#[test]
fn mcp_only_integration_points_to_shim() {
    let snapshot = snapshot_for_store_guidance(WatchServiceStatus::Inactive, "");

    let actions = build_next_actions_with_context(&snapshot, &mcp_only_integration(), runtime());

    assert!(actions
        .iter()
        .any(|a| a.label == "Write agent shim for codex"));
}
