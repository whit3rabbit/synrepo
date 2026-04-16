use tempfile::tempdir;

use super::support::init_synrepo_with_completed_reconcile;
use crate::{
    config::Config,
    core::{
        ids::{ConceptNodeId, EdgeId, FileNodeId, NodeId},
        provenance::{Provenance, SourceRef},
    },
    pipeline::repair::{build_repair_report, DriftClass, RepairAction, RepairSurface},
    store::sqlite::SqliteGraphStore,
    structure::graph::{ConceptNode, Edge, EdgeKind, Epistemic, GraphStore},
};

#[test]
fn check_declared_links_detects_dangling_governs_target() {
    let dir = tempdir().unwrap();
    let synrepo_dir = dir.path().join(".synrepo");
    init_synrepo_with_completed_reconcile(&synrepo_dir);

    let graph_dir = synrepo_dir.join("graph");
    let mut graph = SqliteGraphStore::open(&graph_dir).unwrap();

    let concept_id = ConceptNodeId(0xc0cc_c0cc_c0cc_c0cc);
    let ghost_file_id = FileNodeId(0xdead_beef_1234_5678);

    let provenance = Provenance::structural(
        "test",
        "test-rev",
        vec![SourceRef {
            file_id: None,
            path: "docs/concepts/test.md".to_string(),
            content_hash: "abc123".to_string(),
        }],
    );

    let concept = ConceptNode {
        id: concept_id,
        path: "docs/concepts/test.md".to_string(),
        title: "Test Concept".to_string(),
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
            id: EdgeId(0x1234_5678),
            from: NodeId::Concept(concept_id),
            to: NodeId::File(ghost_file_id),
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
    drop(graph);

    let report = build_repair_report(&synrepo_dir, &Config::default());
    let finding = report
        .findings
        .iter()
        .find(|f| f.surface == RepairSurface::DeclaredLinks)
        .unwrap();

    assert_eq!(finding.drift_class, DriftClass::Stale);
    assert_eq!(finding.recommended_action, RepairAction::RunReconcile);
    assert!(
        finding
            .notes
            .as_deref()
            .unwrap_or("")
            .contains("docs/concepts/test.md"),
        "notes must identify the affected concept doc"
    );
}
