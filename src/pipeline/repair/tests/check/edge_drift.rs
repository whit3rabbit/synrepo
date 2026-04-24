use tempfile::tempdir;

use super::super::support::{init_synrepo, init_synrepo_with_completed_reconcile};
use crate::config::Config;
use crate::core::ids::EdgeId;
use crate::pipeline::repair::{build_repair_report, DriftClass, RepairSurface};
use crate::structure::graph::GraphStore;

#[test]
fn edge_drift_surface_appears_in_report_when_graph_exists() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");
    init_synrepo_with_completed_reconcile(&synrepo_dir);

    // Create a minimal graph store so the edge_drift finding can query it.
    let graph_dir = synrepo_dir.join("graph");
    let graph = crate::store::sqlite::SqliteGraphStore::open(&graph_dir).unwrap();
    // The graph is empty with no drift scores -> Absent (not yet assessed).
    drop(graph);

    let report = build_repair_report(&synrepo_dir, &Config::default());
    let drift_findings: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.surface == RepairSurface::EdgeDrift)
        .collect();
    assert!(
        !drift_findings.is_empty(),
        "edge_drift surface must appear in report"
    );
    assert_eq!(
        drift_findings[0].drift_class,
        DriftClass::Absent,
        "empty graph with no drift scores should report Absent, not Current"
    );
}

#[test]
fn edge_drift_reports_no_high_drift_when_all_scores_below_threshold() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");
    init_synrepo(&synrepo_dir);

    let graph_dir = synrepo_dir.join("graph");
    let mut graph = crate::store::sqlite::SqliteGraphStore::open(&graph_dir).unwrap();

    // Write drift scores that are non-zero but below the 0.7 high-drift threshold.
    let low_scores: Vec<(EdgeId, f32)> = vec![(EdgeId(1), 0.3), (EdgeId(2), 0.5)];
    graph.write_drift_scores(&low_scores, "rev001").unwrap();
    drop(graph);

    let report = build_repair_report(&synrepo_dir, &Config::default());
    let drift_findings: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.surface == RepairSurface::EdgeDrift)
        .collect();

    // With scores below threshold, no HighDriftEdge finding should appear.
    // The surface may appear with Current or not at all, but never HighDriftEdge.
    for finding in &drift_findings {
        assert_ne!(
            finding.drift_class,
            DriftClass::HighDriftEdge,
            "drift scores all < 0.7 should not produce HighDriftEdge"
        );
    }
}
