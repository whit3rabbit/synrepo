use tempfile::tempdir;
use time::OffsetDateTime;

use crate::{
    core::ids::{ConceptNodeId, FileNodeId, NodeId, SymbolNodeId},
    core::provenance::{CreatedBy, Provenance, SourceRef},
    overlay::{
        CitedSpan, ConfidenceTier, CrossLinkFreshness, CrossLinkProvenance, OverlayEdgeKind,
        OverlayEpistemic, OverlayLink, OverlayStore,
    },
    store::{overlay::SqliteOverlayStore, sqlite::SqliteGraphStore},
    structure::graph::{ConceptNode, Epistemic, FileNode, GraphStore, SymbolKind, SymbolNode},
};

use super::findings::{CrossLinkFinding, FindingsFilter};

fn sample_provenance(path: &str, hash: &str) -> Provenance {
    Provenance {
        created_at: OffsetDateTime::UNIX_EPOCH,
        source_revision: "rev".to_string(),
        created_by: CreatedBy::StructuralPipeline,
        pass: "parse".to_string(),
        source_artifacts: vec![SourceRef {
            file_id: None,
            path: path.to_string(),
            content_hash: hash.to_string(),
        }],
    }
}

fn sample_link(
    from: NodeId,
    to: NodeId,
    tier: ConfidenceTier,
    from_hash: &str,
    to_hash: &str,
) -> OverlayLink {
    OverlayLink {
        from,
        to,
        kind: OverlayEdgeKind::References,
        epistemic: OverlayEpistemic::MachineAuthoredLowConf,
        source_spans: vec![CitedSpan {
            artifact: from,
            normalized_text: "authenticate".to_string(),
            verified_at_offset: 0,
            lcs_ratio: 0.94,
        }],
        target_spans: vec![CitedSpan {
            artifact: to,
            normalized_text: "fn authenticate".to_string(),
            verified_at_offset: 0,
            lcs_ratio: 0.93,
        }],
        from_content_hash: from_hash.to_string(),
        to_content_hash: to_hash.to_string(),
        confidence_score: 0.61,
        confidence_tier: tier,
        rationale: Some("match".to_string()),
        provenance: CrossLinkProvenance {
            pass_id: "cross-link-v1".to_string(),
            model_identity: "test-model".to_string(),
            generated_at: OffsetDateTime::UNIX_EPOCH,
        },
    }
}

fn seed_graph(graph: &mut SqliteGraphStore) {
    graph.begin().unwrap();
    graph
        .upsert_file(FileNode {
            id: FileNodeId(1),
            path: "docs/adr/0001-auth.md".to_string(),
            path_history: Vec::new(),
            content_hash: "doc-hash".to_string(),
            size_bytes: 64,
            language: Some("markdown".to_string()),
            inline_decisions: Vec::new(),
            last_observed_rev: None,
            epistemic: Epistemic::ParserObserved,
            provenance: sample_provenance("docs/adr/0001-auth.md", "doc-hash"),
        })
        .unwrap();
    graph
        .upsert_file(FileNode {
            id: FileNodeId(2),
            path: "src/lib.rs".to_string(),
            path_history: Vec::new(),
            content_hash: "code-hash".to_string(),
            size_bytes: 64,
            language: Some("rust".to_string()),
            inline_decisions: Vec::new(),
            last_observed_rev: None,
            epistemic: Epistemic::ParserObserved,
            provenance: sample_provenance("src/lib.rs", "code-hash"),
        })
        .unwrap();
    graph
        .upsert_symbol(SymbolNode {
            id: SymbolNodeId(7),
            file_id: FileNodeId(2),
            qualified_name: "crate::authenticate".to_string(),
            display_name: "authenticate".to_string(),
            kind: SymbolKind::Function,
            body_byte_range: (0, 16),
            body_hash: "body-hash".to_string(),
            signature: Some("fn authenticate()".to_string()),
            doc_comment: None,
            first_seen_rev: None,
            last_modified_rev: None,
            last_observed_rev: None,
            retired_at_rev: None,
            epistemic: Epistemic::ParserObserved,
            provenance: sample_provenance("src/lib.rs", "code-hash"),
        })
        .unwrap();
    graph
        .upsert_concept(ConceptNode {
            id: ConceptNodeId(3),
            path: "docs/adr/0001-auth.md".to_string(),
            title: "Authenticate".to_string(),
            aliases: vec!["authenticate".to_string()],
            summary: Some("auth flow".to_string()),
            status: None,
            decision_body: None,
            last_observed_rev: None,
            epistemic: Epistemic::HumanDeclared,
            provenance: sample_provenance("docs/adr/0001-auth.md", "doc-hash"),
        })
        .unwrap();
    graph.commit().unwrap();
}

#[test]
fn findings_surface_below_threshold_candidates() {
    let dir = tempdir().unwrap();
    let overlay_dir = dir.path().join("overlay");
    let graph_dir = dir.path().join("graph");
    let mut overlay = SqliteOverlayStore::open(&overlay_dir).unwrap();
    let mut graph = SqliteGraphStore::open(&graph_dir).unwrap();

    seed_graph(&mut graph);

    let from = NodeId::Concept(ConceptNodeId(3));
    let to = NodeId::Symbol(SymbolNodeId(7));
    overlay
        .insert_link(sample_link(
            from,
            to,
            ConfidenceTier::BelowThreshold,
            "doc-hash",
            "code-hash",
        ))
        .unwrap();

    let findings = overlay
        .findings(&graph, &FindingsFilter::default())
        .unwrap();

    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].tier, ConfidenceTier::BelowThreshold);
}

#[test]
fn findings_can_filter_to_source_deleted() {
    let dir = tempdir().unwrap();
    let overlay_dir = dir.path().join("overlay");
    let graph_dir = dir.path().join("graph");
    let mut overlay = SqliteOverlayStore::open(&overlay_dir).unwrap();
    let mut graph = SqliteGraphStore::open(&graph_dir).unwrap();

    seed_graph(&mut graph);

    let from = NodeId::File(FileNodeId(1));
    let to = NodeId::Symbol(SymbolNodeId(7));
    overlay
        .insert_link(sample_link(
            from,
            to,
            ConfidenceTier::High,
            "doc-hash",
            "code-hash",
        ))
        .unwrap();

    graph.begin().unwrap();
    graph.delete_node(NodeId::Symbol(SymbolNodeId(7))).unwrap();
    graph.commit().unwrap();

    let findings = overlay
        .findings(
            &graph,
            &FindingsFilter {
                freshness: Some(CrossLinkFreshness::SourceDeleted),
                ..FindingsFilter::default()
            },
        )
        .unwrap();

    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].freshness, CrossLinkFreshness::SourceDeleted);
}

#[allow(dead_code)]
fn _assert_serde(_: &CrossLinkFinding) {}
