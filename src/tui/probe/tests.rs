//! Unit tests for probe view models.

use std::path::PathBuf;

use crate::pipeline::context_metrics::ContextMetrics;
use crate::surface::status_snapshot::{
    CommentaryCoverage, GraphSnapshotStatus, RepairAuditState, StatusSnapshot,
};

use super::{build_health_vm, Severity};

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
