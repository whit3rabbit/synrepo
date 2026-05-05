//! Unit tests for the trust dashboard view model.

use std::path::PathBuf;

use crate::overlay::AgentNoteCounts;
use crate::pipeline::context_metrics::ContextMetrics;
use crate::surface::status_snapshot::{
    CommentaryCoverage, GraphSnapshotStatus, RepairAuditState, StatusSnapshot,
};

use super::{build_trust_vm, Severity};

fn snapshot(metrics: Option<ContextMetrics>, notes: Option<AgentNoteCounts>) -> StatusSnapshot {
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
            display: "unavailable".to_string(),
        },
        agent_note_counts: notes,
        explain_provider: None,
        explain_totals: None,
        context_metrics: metrics,
        last_compaction: None,
        repair_audit: RepairAuditState::Ok,
        recent_activity: None,
        synrepo_dir: PathBuf::from("/tmp/trust-test"),
    }
}

#[test]
fn absent_metrics_render_no_data_not_zero() {
    let vm = build_trust_vm(&snapshot(None, None));
    assert_eq!(vm.context_rows[0].value, "no data");
    assert_eq!(vm.overlay_rows[0].value, "no data");
    assert_eq!(vm.context_rows[0].severity, Severity::Stale);
}

#[test]
fn healthy_metrics_include_context_and_overlay_rows() {
    let mut metrics = ContextMetrics::default();
    metrics.cards_served_total = 4;
    metrics.card_tokens_total = 800;
    metrics.estimated_tokens_saved_total = 7_200;
    metrics.changed_files_total = 2;
    metrics.test_surface_hits_total = 1;
    let notes = AgentNoteCounts {
        active: 3,
        ..AgentNoteCounts::default()
    };

    let vm = build_trust_vm(&snapshot(Some(metrics), Some(notes)));
    assert!(vm.context_rows.iter().any(|r| r.label == "cards served"));
    assert!(vm.context_rows.iter().any(|r| r.label == "mcp requests"));
    assert!(vm.context_rows.iter().any(|r| r.label == "saved context"));
    assert!(vm.overlay_rows.iter().any(|r| r.label == "active"));
    assert!(vm.change_rows.iter().any(|r| r.label == "changed files"));
}

#[test]
fn saved_context_row_counts_explicit_note_writes() {
    let mut metrics = ContextMetrics::default();
    metrics
        .saved_context_writes_total
        .insert("note_add".to_string(), 2);

    let vm = build_trust_vm(&snapshot(Some(metrics), None));
    let saved_row = vm
        .context_rows
        .iter()
        .find(|r| r.label == "saved context")
        .expect("saved context row must be present with metrics");

    assert_eq!(saved_row.value, "2 note writes");
    assert_eq!(saved_row.severity, Severity::Healthy);
}

#[test]
fn stale_and_invalid_notes_degrade_trust() {
    let metrics = ContextMetrics {
        cards_served_total: 1,
        stale_responses_total: 1,
        ..ContextMetrics::default()
    };
    let notes = AgentNoteCounts {
        stale: 1,
        invalid: 1,
        ..AgentNoteCounts::default()
    };

    let vm = build_trust_vm(&snapshot(Some(metrics), Some(notes)));
    assert!(
        vm.degraded_rows
            .iter()
            .any(|r| r.label == "stale responses"),
        "stale responses should appear in degraded summary"
    );
    assert!(
        vm.degraded_rows.iter().any(|r| r.label == "invalid"),
        "invalid notes should appear in degraded summary"
    );
}
