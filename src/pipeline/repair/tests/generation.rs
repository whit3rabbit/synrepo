use tempfile::tempdir;
use time::OffsetDateTime;

use crate::{
    config::Config,
    core::{
        ids::{ConceptNodeId, EdgeId, FileNodeId, NodeId, SymbolNodeId},
        provenance::{CreatedBy, Provenance, SourceRef},
    },
    overlay::{
        CitedSpan, ConfidenceTier, CrossLinkProvenance, OverlayEdgeKind, OverlayEpistemic,
        OverlayLink,
    },
    pipeline::explain::CrossLinkGenerator,
    store::{overlay::SqliteOverlayStore, sqlite::SqliteGraphStore},
    structure::graph::{
        ConceptNode, Edge, EdgeKind, Epistemic, FileNode, GraphStore, SymbolKind, SymbolNode,
        Visibility,
    },
};

use super::super::cross_links::run_cross_link_generation_with_generator;

#[derive(Clone)]
struct FakeGenerator {
    links: Vec<OverlayLink>,
}

impl CrossLinkGenerator for FakeGenerator {
    fn generate_candidates(
        &self,
        scope: &crate::pipeline::explain::CandidateScope,
    ) -> crate::Result<Vec<OverlayLink>> {
        Ok(self
            .links
            .iter()
            .filter(|link| {
                scope.pairs.iter().any(|pair| {
                    pair.from == link.from && pair.to == link.to && pair.kind == link.kind
                })
            })
            .cloned()
            .collect())
    }
}

struct Fixture {
    repo_root: std::path::PathBuf,
    synrepo_dir: std::path::PathBuf,
    links: Vec<OverlayLink>,
}

#[test]
fn generation_persists_verified_candidates_and_audit_rows() {
    let fixture = setup_fixture(false);

    let outcome = run_cross_link_generation_with_generator(
        &fixture.repo_root,
        &fixture.synrepo_dir,
        &Config::default(),
        true,
        false,
        &FakeGenerator {
            links: fixture.links.clone(),
        },
    )
    .unwrap();

    assert_eq!(outcome.inserted, 1);
    assert_eq!(outcome.blocked_pairs, 0);

    let overlay = SqliteOverlayStore::open_existing(&fixture.synrepo_dir.join("overlay")).unwrap();
    let stored = overlay.all_candidates(None).unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].confidence_tier, ConfidenceTier::High);
    assert_eq!(stored[0].source_spans[0].lcs_ratio, 1.0);
    assert_eq!(overlay.cross_link_audit_count().unwrap(), 1);
}

#[test]
fn generation_respects_cost_limit() {
    let fixture = setup_fixture(true);
    let config = Config {
        cross_link_cost_limit: 1,
        ..Config::default()
    };

    let outcome = run_cross_link_generation_with_generator(
        &fixture.repo_root,
        &fixture.synrepo_dir,
        &config,
        true,
        false,
        &FakeGenerator {
            links: fixture.links.clone(),
        },
    )
    .unwrap();

    assert_eq!(outcome.inserted, 1);
    assert_eq!(outcome.blocked_pairs, 1);

    let overlay = SqliteOverlayStore::open_existing(&fixture.synrepo_dir.join("overlay")).unwrap();
    assert_eq!(overlay.cross_link_count().unwrap(), 1);
    assert_eq!(overlay.cross_link_audit_count().unwrap(), 1);
}

fn setup_fixture(with_second_pair: bool) -> Fixture {
    let repo = tempdir().unwrap();
    let repo_root = repo.keep();
    std::fs::create_dir_all(repo_root.join("docs/adr")).unwrap();
    std::fs::create_dir_all(repo_root.join("src")).unwrap();

    let doc_one = "Authenticate flow documents how authenticate works.\n";
    let doc_two = "Authorize flow documents how authorize works.\n";
    std::fs::write(repo_root.join("docs/adr/0001-auth.md"), doc_one).unwrap();
    if with_second_pair {
        std::fs::write(repo_root.join("docs/adr/0002-authorize.md"), doc_two).unwrap();
    }

    let code = if with_second_pair {
        "pub fn authenticate() {}\n\npub fn authorize() {}\n".to_string()
    } else {
        "pub fn authenticate() {}\n".to_string()
    };
    std::fs::write(repo_root.join("src/lib.rs"), &code).unwrap();

    let synrepo_dir = repo_root.join(".synrepo");
    let graph_dir = synrepo_dir.join("graph");
    let mut graph = SqliteGraphStore::open(&graph_dir).unwrap();
    seed_fixture_graph(&mut graph, with_second_pair, &code);

    let mut links = vec![fixture_link(
        NodeId::Concept(ConceptNodeId(1)),
        NodeId::Symbol(SymbolNodeId(1)),
        "authenticate flow documents how authenticate works",
        "pub fn authenticate() {}",
    )];
    if with_second_pair {
        links.push(fixture_link(
            NodeId::Concept(ConceptNodeId(2)),
            NodeId::Symbol(SymbolNodeId(2)),
            "authorize flow documents how authorize works",
            "pub fn authorize() {}",
        ));
    }

    Fixture {
        repo_root,
        synrepo_dir,
        links,
    }
}

fn seed_fixture_graph(graph: &mut SqliteGraphStore, with_second_pair: bool, code: &str) {
    graph.begin().unwrap();
    graph
        .upsert_file(FileNode {
            id: FileNodeId(1),
            root_id: "primary".to_string(),
            path: "docs/adr/0001-auth.md".to_string(),
            path_history: Vec::new(),
            content_hash: "doc-hash-1".to_string(),
            size_bytes: 64,
            language: Some("markdown".to_string()),
            inline_decisions: Vec::new(),
            last_observed_rev: None,
            epistemic: Epistemic::ParserObserved,
            provenance: provenance("docs/adr/0001-auth.md", "doc-hash-1"),
        })
        .unwrap();
    graph
        .upsert_file(FileNode {
            id: FileNodeId(10),
            root_id: "primary".to_string(),
            path: "src/lib.rs".to_string(),
            path_history: Vec::new(),
            content_hash: "code-hash".to_string(),
            size_bytes: code.len() as u64,
            language: Some("rust".to_string()),
            inline_decisions: Vec::new(),
            last_observed_rev: None,
            epistemic: Epistemic::ParserObserved,
            provenance: provenance("src/lib.rs", "code-hash"),
        })
        .unwrap();
    graph
        .upsert_symbol(SymbolNode {
            id: SymbolNodeId(1),
            file_id: FileNodeId(10),
            qualified_name: "crate::authenticate".to_string(),
            display_name: "authenticate".to_string(),
            kind: SymbolKind::Function,
            visibility: Visibility::Public,
            body_byte_range: body_range(code, "pub fn authenticate() {}"),
            body_hash: "body-auth".to_string(),
            signature: Some("pub fn authenticate()".to_string()),
            doc_comment: None,
            first_seen_rev: None,
            last_modified_rev: None,
            last_observed_rev: None,
            retired_at_rev: None,
            epistemic: Epistemic::ParserObserved,
            provenance: provenance("src/lib.rs", "code-hash"),
        })
        .unwrap();
    graph
        .upsert_concept(ConceptNode {
            id: ConceptNodeId(1),
            path: "docs/adr/0001-auth.md".to_string(),
            title: "Authenticate Flow".to_string(),
            aliases: vec!["authenticate".to_string()],
            summary: Some("authenticate works".to_string()),
            status: None,
            decision_body: None,
            last_observed_rev: None,
            epistemic: Epistemic::HumanDeclared,
            provenance: provenance("docs/adr/0001-auth.md", "doc-hash-1"),
        })
        .unwrap();
    graph
        .insert_edge(Edge {
            id: EdgeId(1),
            from: NodeId::File(FileNodeId(10)),
            to: NodeId::Symbol(SymbolNodeId(1)),
            kind: EdgeKind::Defines,
            owner_file_id: None,
            last_observed_rev: None,
            retired_at_rev: None,
            epistemic: Epistemic::ParserObserved,
            drift_score: 0.0,
            provenance: provenance("src/lib.rs", "code-hash"),
        })
        .unwrap();
    graph
        .insert_edge(Edge {
            id: EdgeId(2),
            from: NodeId::Concept(ConceptNodeId(1)),
            to: NodeId::File(FileNodeId(10)),
            kind: EdgeKind::Governs,
            owner_file_id: None,
            last_observed_rev: None,
            retired_at_rev: None,
            epistemic: Epistemic::HumanDeclared,
            drift_score: 0.0,
            provenance: provenance("docs/adr/0001-auth.md", "doc-hash-1"),
        })
        .unwrap();

    if with_second_pair {
        graph
            .upsert_file(FileNode {
                id: FileNodeId(2),
                root_id: "primary".to_string(),
                path: "docs/adr/0002-authorize.md".to_string(),
                path_history: Vec::new(),
                content_hash: "doc-hash-2".to_string(),
                size_bytes: 64,
                language: Some("markdown".to_string()),
                inline_decisions: Vec::new(),
                last_observed_rev: None,
                epistemic: Epistemic::ParserObserved,
                provenance: provenance("docs/adr/0002-authorize.md", "doc-hash-2"),
            })
            .unwrap();
        graph
            .upsert_symbol(SymbolNode {
                id: SymbolNodeId(2),
                file_id: FileNodeId(10),
                qualified_name: "crate::authorize".to_string(),
                display_name: "authorize".to_string(),
                kind: SymbolKind::Function,
                visibility: Visibility::Public,
                body_byte_range: body_range(code, "pub fn authorize() {}"),
                body_hash: "body-authorize".to_string(),
                signature: Some("pub fn authorize()".to_string()),
                doc_comment: None,
                first_seen_rev: None,
                last_modified_rev: None,
                last_observed_rev: None,
                retired_at_rev: None,
                epistemic: Epistemic::ParserObserved,
                provenance: provenance("src/lib.rs", "code-hash"),
            })
            .unwrap();
        graph
            .upsert_concept(ConceptNode {
                id: ConceptNodeId(2),
                path: "docs/adr/0002-authorize.md".to_string(),
                title: "Authorize Flow".to_string(),
                aliases: vec!["authorize".to_string()],
                summary: Some("authorize works".to_string()),
                status: None,
                decision_body: None,
                last_observed_rev: None,
                epistemic: Epistemic::HumanDeclared,
                provenance: provenance("docs/adr/0002-authorize.md", "doc-hash-2"),
            })
            .unwrap();
        graph
            .insert_edge(Edge {
                id: EdgeId(3),
                from: NodeId::File(FileNodeId(10)),
                to: NodeId::Symbol(SymbolNodeId(2)),
                kind: EdgeKind::Defines,
                owner_file_id: None,
                last_observed_rev: None,
                retired_at_rev: None,
                epistemic: Epistemic::ParserObserved,
                drift_score: 0.0,
                provenance: provenance("src/lib.rs", "code-hash"),
            })
            .unwrap();
        graph
            .insert_edge(Edge {
                id: EdgeId(4),
                from: NodeId::Concept(ConceptNodeId(2)),
                to: NodeId::File(FileNodeId(10)),
                kind: EdgeKind::Governs,
                owner_file_id: None,
                last_observed_rev: None,
                retired_at_rev: None,
                epistemic: Epistemic::HumanDeclared,
                drift_score: 0.0,
                provenance: provenance("docs/adr/0002-authorize.md", "doc-hash-2"),
            })
            .unwrap();
    }

    graph.commit().unwrap();
}

fn fixture_link(from: NodeId, to: NodeId, source_span: &str, target_span: &str) -> OverlayLink {
    OverlayLink {
        from,
        to,
        kind: OverlayEdgeKind::References,
        epistemic: OverlayEpistemic::MachineAuthoredLowConf,
        source_spans: vec![CitedSpan {
            artifact: from,
            normalized_text: source_span.to_string(),
            verified_at_offset: 0,
            lcs_ratio: 0.1,
        }],
        target_spans: vec![CitedSpan {
            artifact: to,
            normalized_text: target_span.to_string(),
            verified_at_offset: 0,
            lcs_ratio: 0.1,
        }],
        from_content_hash: String::new(),
        to_content_hash: String::new(),
        confidence_score: 0.0,
        confidence_tier: ConfidenceTier::BelowThreshold,
        rationale: Some("fixture".to_string()),
        provenance: CrossLinkProvenance {
            pass_id: "cross-link-v1".to_string(),
            model_identity: "fake-generator".to_string(),
            generated_at: OffsetDateTime::UNIX_EPOCH,
        },
    }
}

fn body_range(source: &str, snippet: &str) -> (u32, u32) {
    let start = source.find(snippet).unwrap();
    ((start as u32), (start + snippet.len()) as u32)
}

fn provenance(path: &str, hash: &str) -> Provenance {
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
