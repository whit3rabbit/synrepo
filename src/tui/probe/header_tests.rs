use std::{collections::BTreeMap, path::PathBuf};

use crate::bootstrap::runtime_probe::{AgentIntegration, AgentTargetKind};
use crate::pipeline::diagnostics::{
    EmbeddingHealth, ReconcileHealth, RuntimeDiagnostics, WriterStatus,
};
use crate::pipeline::watch::WatchServiceStatus;
use crate::store::sqlite::PersistedGraphStats;
use crate::surface::status_snapshot::{
    CommentaryCoverage, ExportState, ExportStatus, GraphSnapshotStatus, OverlayState,
    RepairAuditState, StatusSnapshot,
};

use super::{build_header_vm, Severity};

fn snapshot() -> StatusSnapshot {
    StatusSnapshot {
        initialized: true,
        config: None,
        diagnostics: Some(RuntimeDiagnostics {
            reconcile_health: ReconcileHealth::Current,
            watch_status: WatchServiceStatus::Inactive,
            writer_status: WriterStatus::Free,
            store_guidance: Vec::new(),
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

#[test]
fn header_labels_mcp_only_as_shim_missing() {
    let header = build_header_vm(
        "~/repo".to_string(),
        &snapshot(),
        &AgentIntegration::McpOnly {
            target: AgentTargetKind::Codex,
        },
        None,
    );

    assert_eq!(header.mcp_label, "shim missing (codex)");
    assert_eq!(header.mcp_severity, Severity::Stale);
}
