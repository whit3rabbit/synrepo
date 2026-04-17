use tempfile::tempdir;

use super::support::{init_synrepo, init_synrepo_with_completed_reconcile, write_foreign_lock};
use crate::{
    config::Config,
    pipeline::watch::{persist_reconcile_state, ReconcileOutcome},
};

use crate::pipeline::repair::{build_repair_report, RepairAction, RepairSurface, Severity};

#[test]
fn check_on_fresh_initialized_runtime_has_no_actionable_findings() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");
    init_synrepo_with_completed_reconcile(&synrepo_dir);

    let report = build_repair_report(&synrepo_dir, &Config::default());
    assert!(
        !report.has_actionable(),
        "fresh runtime should have no actionable findings, got: {:?}",
        report
            .findings
            .iter()
            .filter(|f| f.severity == Severity::Actionable
                && f.recommended_action != RepairAction::None)
            .map(|f| f.surface.as_str())
            .collect::<Vec<_>>()
    );
    assert!(!report.has_blocked());
}

#[test]
fn check_detects_stale_reconcile() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");
    init_synrepo(&synrepo_dir);

    persist_reconcile_state(
        &synrepo_dir,
        &ReconcileOutcome::Failed("disk error".to_string()),
        0,
    );

    let report = build_repair_report(&synrepo_dir, &Config::default());
    let structural = report
        .findings
        .iter()
        .find(|f| f.surface == RepairSurface::StructuralRefresh)
        .unwrap();

    assert_eq!(structural.drift_class.as_str(), "stale");
    assert_eq!(structural.recommended_action, RepairAction::RunReconcile);
}

#[test]
fn check_reports_blocked_drift_when_writer_lock_held_by_foreign_pid() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");
    init_synrepo_with_completed_reconcile(&synrepo_dir);
    write_foreign_lock(&synrepo_dir);

    let report = build_repair_report(&synrepo_dir, &Config::default());
    let lock_finding = report
        .findings
        .iter()
        .find(|f| f.surface == RepairSurface::WriterLock)
        .unwrap();

    assert!(
        matches!(
            lock_finding.severity,
            Severity::Blocked | Severity::Actionable
        ),
        "writer lock finding must be Blocked or Actionable, got {:?}",
        lock_finding.severity,
    );
}

#[test]
fn check_reports_stale_rationale_when_no_drift_assessment() {
    use crate::pipeline::repair::{DriftClass, RepairAction};

    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");
    init_synrepo(&synrepo_dir);

    let report = build_repair_report(&synrepo_dir, &Config::default());

    let finding = report
        .findings
        .iter()
        .find(|f| f.surface == RepairSurface::StaleRationale)
        .expect("stale_rationale must be in report");
    // Graph not materialized yet: Absent / ReportOnly / NotSupported action.
    assert_eq!(finding.drift_class, DriftClass::Absent);
    assert_eq!(finding.severity, Severity::ReportOnly);
    assert_eq!(finding.recommended_action, RepairAction::None);

    // ExportSurface is now implemented; no manifest means Absent/ReportOnly.
    let export_finding = report
        .findings
        .iter()
        .find(|f| f.surface == RepairSurface::ExportSurface)
        .expect("ExportSurface must be in report");
    assert_eq!(export_finding.drift_class, DriftClass::Absent);
    assert_eq!(export_finding.recommended_action, RepairAction::None);
    assert_eq!(export_finding.severity, Severity::ReportOnly);
}

#[test]
fn check_reports_commentary_overlay_absent_when_no_overlay_db() {
    use crate::pipeline::repair::{DriftClass, RepairAction};
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");
    init_synrepo(&synrepo_dir);

    let report = build_repair_report(&synrepo_dir, &Config::default());
    let finding = report
        .findings
        .iter()
        .find(|f| f.surface == RepairSurface::CommentaryOverlayEntries)
        .expect("commentary overlay finding must be present");

    assert_eq!(finding.drift_class, DriftClass::Absent);
    assert_eq!(finding.recommended_action, RepairAction::None);
}

#[test]
fn check_reports_cross_link_overlay_absent_when_no_overlay_db() {
    use crate::pipeline::repair::{DriftClass, RepairAction};
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");
    init_synrepo(&synrepo_dir);

    let report = build_repair_report(&synrepo_dir, &Config::default());
    let finding = report
        .findings
        .iter()
        .find(|f| f.surface == RepairSurface::ProposedLinksOverlay)
        .expect("proposed links overlay finding must be present");

    assert_eq!(finding.drift_class, DriftClass::Absent);
    assert_eq!(finding.recommended_action, RepairAction::None);
    assert_eq!(finding.severity, Severity::ReportOnly);
}

#[test]
fn check_reports_source_deleted_when_endpoints_missing_from_graph() {
    use crate::core::ids::{ConceptNodeId, NodeId, SymbolNodeId};
    use crate::overlay::{
        CitedSpan, ConfidenceTier, CrossLinkProvenance, OverlayEdgeKind, OverlayEpistemic,
        OverlayLink, OverlayStore,
    };
    use crate::pipeline::repair::{DriftClass, RepairAction};
    use crate::store::overlay::SqliteOverlayStore;
    use crate::store::sqlite::SqliteGraphStore;
    use time::OffsetDateTime;

    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");
    init_synrepo(&synrepo_dir);

    // Materialize the graph DB (empty) and overlay DB; seed a link whose
    // endpoints do not exist in the graph.
    drop(SqliteGraphStore::open(&synrepo_dir.join("graph")).unwrap());
    let mut overlay = SqliteOverlayStore::open(&synrepo_dir.join("overlay")).unwrap();
    let from = NodeId::Concept(ConceptNodeId(777));
    let to = NodeId::Symbol(SymbolNodeId(888));
    let link = OverlayLink {
        from,
        to,
        kind: OverlayEdgeKind::References,
        epistemic: OverlayEpistemic::MachineAuthoredHighConf,
        source_spans: vec![CitedSpan {
            artifact: from,
            normalized_text: "login".into(),
            verified_at_offset: 0,
            lcs_ratio: 0.95,
        }],
        target_spans: vec![CitedSpan {
            artifact: to,
            normalized_text: "fn login".into(),
            verified_at_offset: 0,
            lcs_ratio: 1.0,
        }],
        from_content_hash: "stored-from".into(),
        to_content_hash: "stored-to".into(),
        confidence_score: 0.9,
        confidence_tier: ConfidenceTier::High,
        rationale: None,
        provenance: CrossLinkProvenance {
            pass_id: "cross-link-v1".into(),
            model_identity: "claude-sonnet-4-6".into(),
            generated_at: OffsetDateTime::from_unix_timestamp(1_712_000_000).unwrap(),
        },
    };
    overlay.insert_link(link).unwrap();
    drop(overlay);

    let report = build_repair_report(&synrepo_dir, &Config::default());
    let finding = report
        .findings
        .iter()
        .find(|f| f.surface == RepairSurface::ProposedLinksOverlay)
        .expect("proposed links overlay finding must be present");

    assert_eq!(finding.drift_class, DriftClass::SourceDeleted);
    assert_eq!(finding.recommended_action, RepairAction::ManualReview);
    assert_eq!(finding.severity, Severity::ReportOnly);
}

#[test]
fn check_report_render_includes_all_surfaces() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");
    init_synrepo(&synrepo_dir);

    let report = build_repair_report(&synrepo_dir, &Config::default());
    let rendered = report.render();

    for surface in [
        "store_maintenance",
        "structural_refresh",
        "writer_lock",
        "declared_links",
        "commentary_overlay_entries",
        "proposed_links_overlay",
        "export_surface",
    ] {
        assert!(
            rendered.contains(surface),
            "render must mention surface {surface}"
        );
    }
}

#[test]
fn check_report_serializes_to_valid_json() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");
    init_synrepo_with_completed_reconcile(&synrepo_dir);

    let report = build_repair_report(&synrepo_dir, &Config::default());
    let json = serde_json::to_string_pretty(&report).unwrap();
    let decoded: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert!(decoded["checked_at"].is_string());
    assert!(decoded["findings"].is_array());
    assert!(
        decoded["findings"].as_array().unwrap().len() >= 6,
        "report must include at least 6 surfaces"
    );
}

#[test]
fn edge_drift_surface_appears_in_report_when_graph_exists() {
    use crate::pipeline::repair::DriftClass;
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
    use crate::core::ids::EdgeId;
    use crate::pipeline::repair::DriftClass;
    use crate::structure::graph::GraphStore;

    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");
    init_synrepo_with_completed_reconcile(&synrepo_dir);

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

#[test]
fn check_surfaces_pending_promotion_as_actionable() {
    use crate::core::ids::{ConceptNodeId, NodeId, SymbolNodeId};
    use crate::overlay::{
        CitedSpan, ConfidenceTier, CrossLinkProvenance, OverlayEdgeKind, OverlayEpistemic,
        OverlayLink, OverlayStore,
    };
    use crate::pipeline::repair::DriftClass;
    use crate::store::overlay::SqliteOverlayStore;
    use crate::store::sqlite::SqliteGraphStore;
    use time::OffsetDateTime;

    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");
    init_synrepo(&synrepo_dir);

    // Materialize the graph DB (empty) and overlay DB, then seed a link and
    // regress its state to pending_promotion to simulate a crash.
    drop(SqliteGraphStore::open(&synrepo_dir.join("graph")).unwrap());
    let mut overlay = SqliteOverlayStore::open(&synrepo_dir.join("overlay")).unwrap();
    let from = NodeId::Concept(ConceptNodeId(777));
    let to = NodeId::Symbol(SymbolNodeId(888));
    let link = OverlayLink {
        from,
        to,
        kind: OverlayEdgeKind::References,
        epistemic: OverlayEpistemic::MachineAuthoredHighConf,
        source_spans: vec![CitedSpan {
            artifact: from,
            normalized_text: "login".into(),
            verified_at_offset: 0,
            lcs_ratio: 0.95,
        }],
        target_spans: vec![CitedSpan {
            artifact: to,
            normalized_text: "fn login".into(),
            verified_at_offset: 0,
            lcs_ratio: 1.0,
        }],
        from_content_hash: "stored-from".into(),
        to_content_hash: "stored-to".into(),
        confidence_score: 0.9,
        confidence_tier: ConfidenceTier::High,
        rationale: None,
        provenance: CrossLinkProvenance {
            pass_id: "cross-link-v1".into(),
            model_identity: "claude-sonnet-4-6".into(),
            generated_at: OffsetDateTime::from_unix_timestamp(1_712_000_000).unwrap(),
        },
    };
    overlay.insert_link(link).unwrap();
    drop(overlay);

    // Simulate crash: set state to pending_promotion.
    let conn =
        rusqlite::Connection::open(SqliteOverlayStore::db_path(&synrepo_dir.join("overlay")))
            .unwrap();
    conn.execute("UPDATE cross_links SET state = 'pending_promotion'", [])
        .unwrap();
    drop(conn);

    let report = build_repair_report(&synrepo_dir, &Config::default());
    let finding = report
        .findings
        .iter()
        .find(|f| {
            f.surface == RepairSurface::ProposedLinksOverlay
                && f.notes
                    .as_ref()
                    .is_some_and(|n| n.contains("pending_promotion"))
        })
        .expect("pending_promotion finding must be present");

    assert_eq!(finding.drift_class, DriftClass::Stale);
    assert_eq!(finding.severity, Severity::Actionable);
    assert!(
        finding
            .notes
            .as_ref()
            .unwrap()
            .contains("pending_promotion"),
        "finding must mention pending_promotion: {:?}",
        finding.notes
    );
}
