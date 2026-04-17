use tempfile::tempdir;

use super::support::init_synrepo_with_completed_reconcile;
use crate::{
    config::Config,
    core::{
        ids::{ConceptNodeId, EdgeId, FileNodeId, NodeId},
        provenance::{Provenance, SourceRef},
    },
    pipeline::repair::{build_repair_report, DriftClass, RepairAction, RepairSurface, Severity},
    store::sqlite::SqliteGraphStore,
    structure::graph::{ConceptNode, Edge, EdgeKind, Epistemic, GraphStore},
};

fn seed_concept_with_governs_edge(
    graph: &mut SqliteGraphStore,
    concept_id: ConceptNodeId,
    target_file_id: FileNodeId,
    edge_id: EdgeId,
) {
    let provenance = Provenance::structural(
        "test",
        "test-rev",
        vec![SourceRef {
            file_id: None,
            path: "docs/adr/0001-rationale.md".to_string(),
            content_hash: "abc123".to_string(),
        }],
    );
    let concept = ConceptNode {
        id: concept_id,
        path: "docs/adr/0001-rationale.md".to_string(),
        title: "Rationale".to_string(),
        aliases: vec![],
        summary: None,
        status: None,
        decision_body: None,
        last_observed_rev: None,
        epistemic: Epistemic::HumanDeclared,
        provenance: provenance.clone(),
    };
    graph.begin().unwrap();
    graph.upsert_concept(concept).unwrap();
    graph
        .insert_edge(Edge {
            id: edge_id,
            from: NodeId::Concept(concept_id),
            to: NodeId::File(target_file_id),
            kind: EdgeKind::Governs,
            owner_file_id: None,
            last_observed_rev: None,
            retired_at_rev: None,
            epistemic: Epistemic::HumanDeclared,
            drift_score: 0.0,
            provenance,
        })
        .unwrap();
    graph.commit().unwrap();
}

#[test]
fn stale_rationale_reports_absent_when_graph_not_materialized() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");
    init_synrepo_with_completed_reconcile(&synrepo_dir);

    let report = build_repair_report(&synrepo_dir, &Config::default());
    let finding = report
        .findings
        .iter()
        .find(|f| f.surface == RepairSurface::StaleRationale)
        .expect("stale_rationale must be in report");
    assert_eq!(finding.drift_class, DriftClass::Absent);
    assert_eq!(finding.severity, Severity::ReportOnly);
    assert_eq!(finding.recommended_action, RepairAction::None);
}

#[test]
fn stale_rationale_reports_current_when_no_concept_nodes() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");
    init_synrepo_with_completed_reconcile(&synrepo_dir);

    let graph_dir = synrepo_dir.join("graph");
    let mut graph = SqliteGraphStore::open(&graph_dir).unwrap();
    let scores: Vec<(EdgeId, f32)> = vec![(EdgeId(1), 0.2)];
    graph.write_drift_scores(&scores, "rev001").unwrap();
    drop(graph);

    let report = build_repair_report(&synrepo_dir, &Config::default());
    let finding = report
        .findings
        .iter()
        .find(|f| f.surface == RepairSurface::StaleRationale)
        .expect("stale_rationale must be in report");
    assert_eq!(finding.drift_class, DriftClass::Current);
    assert_eq!(finding.severity, Severity::ReportOnly);
}

#[test]
fn stale_rationale_reports_current_when_governed_targets_below_threshold() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");
    init_synrepo_with_completed_reconcile(&synrepo_dir);

    let graph_dir = synrepo_dir.join("graph");
    let mut graph = SqliteGraphStore::open(&graph_dir).unwrap();
    let concept_id = ConceptNodeId(0xc0cc_c0cc_c0cc_c0cc);
    let file_id = FileNodeId(0x1111_1111_1111_1111);
    let edge_id = EdgeId(0x1234);
    seed_concept_with_governs_edge(&mut graph, concept_id, file_id, edge_id);
    graph
        .write_drift_scores(&[(edge_id, 0.3)], "rev001")
        .unwrap();
    drop(graph);

    let report = build_repair_report(&synrepo_dir, &Config::default());
    let finding = report
        .findings
        .iter()
        .find(|f| f.surface == RepairSurface::StaleRationale)
        .expect("stale_rationale must be in report");
    assert_eq!(finding.drift_class, DriftClass::Current);
    assert_eq!(finding.severity, Severity::ReportOnly);
}

#[test]
fn stale_rationale_flags_concept_when_governed_target_drifted() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");
    init_synrepo_with_completed_reconcile(&synrepo_dir);

    let graph_dir = synrepo_dir.join("graph");
    let mut graph = SqliteGraphStore::open(&graph_dir).unwrap();
    let concept_id = ConceptNodeId(0xc0cc_c0cc_c0cc_c0cc);
    let file_id = FileNodeId(0x1111_1111_1111_1111);
    let edge_id = EdgeId(0x1234);
    seed_concept_with_governs_edge(&mut graph, concept_id, file_id, edge_id);
    graph
        .write_drift_scores(&[(edge_id, 0.8)], "rev001")
        .unwrap();
    drop(graph);

    let report = build_repair_report(&synrepo_dir, &Config::default());
    let finding = report
        .findings
        .iter()
        .find(|f| f.surface == RepairSurface::StaleRationale && f.drift_class == DriftClass::Stale)
        .expect("stale_rationale must emit a Stale finding for drifted governed target");

    assert_eq!(finding.severity, Severity::ReportOnly);
    assert_eq!(finding.recommended_action, RepairAction::ManualReview);
    assert_eq!(
        finding.target_id.as_deref(),
        Some("docs/adr/0001-rationale.md"),
        "target_id must identify the drifted concept document"
    );
    let notes = finding.notes.as_deref().unwrap_or("");
    assert!(
        notes.contains("1 of 1"),
        "notes must report the drifted/total ratio: {notes}"
    );
}
