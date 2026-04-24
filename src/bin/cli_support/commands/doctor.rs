//! `synrepo doctor` — narrow view over the shared status snapshot that reports
//! only components whose severity is not `Healthy`. Exits non-zero when any
//! degraded component is present so it is usable in CI and pre-commit hooks.
//!
//! The view reuses `status_snapshot::build_status_snapshot` and the probe view
//! models (`tui::probe::build_header_vm`, `tui::probe::build_health_vm`) to
//! guarantee doctor and the dashboard never disagree about what is degraded.

use std::path::Path;

use serde::Serialize;
use synrepo::bootstrap::runtime_probe::probe;
use synrepo::surface::status_snapshot::{build_status_snapshot, StatusOptions, StatusSnapshot};
use synrepo::tui::probe::{build_header_vm, build_health_vm, display_repo_path, Severity};

#[derive(Debug, Serialize)]
struct DegradedRow {
    label: String,
    value: String,
    severity: &'static str,
}

#[derive(Serialize)]
struct DoctorReport {
    healthy: bool,
    degraded: Vec<DegradedRow>,
}

pub(crate) fn doctor(repo_root: &Path, json: bool) -> anyhow::Result<()> {
    let report = run_doctor(repo_root);
    let degraded_count = report.degraded.len();
    let rendered = if json {
        serde_json::to_string_pretty(&report)? + "\n"
    } else {
        render_text(&report)
    };
    print!("{rendered}");
    if degraded_count > 0 {
        std::process::exit(1);
    }
    Ok(())
}

fn run_doctor(repo_root: &Path) -> DoctorReport {
    let snapshot = build_status_snapshot(
        repo_root,
        StatusOptions {
            recent: false,
            full: false,
        },
    );
    let integration = probe(repo_root).agent_integration;
    let rows = collect_degraded_rows(&snapshot, &integration);
    DoctorReport {
        healthy: rows.is_empty(),
        degraded: rows,
    }
}

fn collect_degraded_rows(
    snapshot: &StatusSnapshot,
    integration: &synrepo::bootstrap::runtime_probe::AgentIntegration,
) -> Vec<DegradedRow> {
    let header = build_header_vm(
        display_repo_path(&snapshot.synrepo_dir),
        snapshot,
        integration,
    );
    let health = build_health_vm(snapshot);

    let mut out: Vec<DegradedRow> = Vec::new();
    let header_rows = [
        (
            "reconcile",
            header.reconcile_label,
            header.reconcile_severity,
        ),
        ("watch", header.watch_label, header.watch_severity),
        ("writer lock", header.lock_label, header.lock_severity),
        ("agent integration", header.mcp_label, header.mcp_severity),
    ];
    for (label, value, severity) in header_rows {
        push_if_degraded(&mut out, label.to_string(), value, severity);
    }
    for row in health.rows {
        push_if_degraded(&mut out, row.label, row.value, row.severity);
    }
    out
}

fn push_if_degraded(out: &mut Vec<DegradedRow>, label: String, value: String, severity: Severity) {
    if !matches!(severity, Severity::Healthy) {
        out.push(DegradedRow {
            label,
            value,
            severity: severity.as_str(),
        });
    }
}

fn render_text(report: &DoctorReport) -> String {
    use std::fmt::Write as _;

    let mut out = String::new();
    if report.healthy {
        out.push_str("synrepo doctor: all components healthy\n");
        return out;
    }
    writeln!(
        out,
        "synrepo doctor: {} degraded component(s)",
        report.degraded.len()
    )
    .unwrap();
    for row in &report.degraded {
        writeln!(out, "  [{}] {} = {}", row.severity, row.label, row.value).unwrap();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use synrepo::bootstrap::runtime_probe::AgentIntegration;
    use synrepo::pipeline::context_metrics::ContextMetrics;
    use synrepo::surface::status_snapshot::{
        CommentaryCoverage, GraphSnapshotStatus, RepairAuditState,
    };

    fn fixture_snapshot(stale_responses: u64, explain: Option<SnapshotExplain>) -> StatusSnapshot {
        let _ = explain;
        let mut metrics = ContextMetrics::default();
        metrics.cards_served_total = 1;
        metrics.stale_responses_total = stale_responses;
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
            context_metrics: Some(metrics),
            last_compaction: None,
            repair_audit: RepairAuditState::Ok,
            recent_activity: None,
            synrepo_dir: PathBuf::from("/tmp/doctor-test"),
        }
    }

    struct SnapshotExplain;

    #[test]
    fn healthy_rows_are_filtered_out() {
        // The minimal fixture has diagnostics=None and graph_stats=None, so
        // header rows (reconcile/watch/lock) and the graph row escalate to
        // Stale/Blocked. We only assert that positive signals stay out of
        // the degraded list: zero stale_responses keeps its row Healthy, and
        // `tokens avoided` is always Healthy.
        let snapshot = fixture_snapshot(0, None);
        let integration = AgentIntegration::Complete {
            target: synrepo::bootstrap::runtime_probe::AgentTargetKind::Claude,
        };
        let rows = collect_degraded_rows(&snapshot, &integration);
        assert!(
            rows.iter().all(|r| r.label != "tokens avoided"),
            "Healthy `tokens avoided` row must not appear in degraded list"
        );
        assert!(
            rows.iter().all(|r| r.label != "stale responses"),
            "Zero stale_responses must stay Healthy and out of the degraded list"
        );
    }

    #[test]
    fn stale_responses_surface_in_doctor_output() {
        let snapshot = fixture_snapshot(5, None);
        let integration = AgentIntegration::Complete {
            target: synrepo::bootstrap::runtime_probe::AgentTargetKind::Claude,
        };
        let rows = collect_degraded_rows(&snapshot, &integration);
        assert!(
            rows.iter().any(|r| r.label == "stale responses"),
            "non-zero stale_responses_total must appear in doctor degraded list, got: {rows:?}"
        );
    }

    #[test]
    fn absent_agent_integration_surfaces_as_stale() {
        let snapshot = fixture_snapshot(0, None);
        let integration = AgentIntegration::Absent;
        let rows = collect_degraded_rows(&snapshot, &integration);
        let agent_row = rows
            .iter()
            .find(|r| r.label == "agent integration")
            .expect("absent integration must surface in doctor output");
        assert_eq!(agent_row.severity, "stale");
    }
}
