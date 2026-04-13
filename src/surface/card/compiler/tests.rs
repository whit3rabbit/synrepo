use super::*;
use crate::{
    config::Config,
    core::ids::NodeId,
    overlay::{CommentaryEntry, CommentaryProvenance, OverlayStore},
    pipeline::structural::run_structural_compile,
    pipeline::synthesis::{CommentaryGenerator, NoOpGenerator},
    store::{overlay::SqliteOverlayStore, sqlite::SqliteGraphStore},
    structure::graph::EdgeKind,
    surface::card::Freshness,
};
use insta::assert_snapshot;
use parking_lot::Mutex;
use std::fs;
use std::sync::Arc;
use tempfile::tempdir;
use time::OffsetDateTime;

fn make_compiler(graph: SqliteGraphStore, repo: &tempfile::TempDir) -> GraphCardCompiler {
    GraphCardCompiler::new(Box::new(graph), Some(repo.path()))
}

fn bootstrap(repo: &tempfile::TempDir) -> SqliteGraphStore {
    let graph_dir = repo.path().join(".synrepo/graph");
    let mut graph = SqliteGraphStore::open(&graph_dir).unwrap();
    run_structural_compile(repo.path(), &Config::default(), &mut graph).unwrap();
    graph
}

#[test]
fn file_card_returns_defined_symbols() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(
        repo.path().join("src/lib.rs"),
        "pub fn foo() {}\npub fn bar() {}\n",
    )
    .unwrap();

    let graph = bootstrap(&repo);
    let file_id = graph.file_by_path("src/lib.rs").unwrap().unwrap().id;
    let compiler = make_compiler(graph, &repo);

    let card = compiler.file_card(file_id, Budget::Tiny).unwrap();
    assert_eq!(card.path, "src/lib.rs");
    assert_eq!(card.symbols.len(), 2);
    let names: Vec<&str> = card
        .symbols
        .iter()
        .map(|s| s.qualified_name.as_str())
        .collect();
    assert!(names.contains(&"foo"), "expected foo in {names:?}");
    assert!(names.contains(&"bar"), "expected bar in {names:?}");
}

#[test]
fn resolve_target_finds_by_path_and_by_name() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(repo.path().join("src/lib.rs"), "pub fn my_func() {}\n").unwrap();

    let graph = bootstrap(&repo);
    let compiler = make_compiler(graph, &repo);

    let by_path = compiler.resolve_target("src/lib.rs").unwrap();
    assert!(matches!(by_path, Some(NodeId::File(_))));

    let by_name = compiler.resolve_target("my_func").unwrap();
    assert!(matches!(by_name, Some(NodeId::Symbol(_))));

    assert!(compiler.resolve_target("nonexistent").unwrap().is_none());
}

#[test]
fn symbol_card_tiny_has_no_source_body() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(
        repo.path().join("src/lib.rs"),
        "/// Docs.\npub fn documented() -> u32 { 42 }\n",
    )
    .unwrap();

    let graph = bootstrap(&repo);
    let file = graph.file_by_path("src/lib.rs").unwrap().unwrap();
    let sym_edge = graph
        .outbound(NodeId::File(file.id), Some(EdgeKind::Defines))
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    let sym_id = match sym_edge.to {
        NodeId::Symbol(id) => id,
        _ => panic!("expected symbol"),
    };
    let compiler = make_compiler(graph, &repo);

    let tiny = compiler.symbol_card(sym_id, Budget::Tiny).unwrap();
    assert_eq!(tiny.name, "documented");
    assert!(
        tiny.source_body.is_none(),
        "tiny budget must not include source body"
    );
    assert!(tiny.approx_tokens > 0);
    assert_eq!(tiny.source_store, SourceStore::Graph);

    let graph2 = {
        let graph_dir = repo.path().join(".synrepo/graph");
        SqliteGraphStore::open(&graph_dir).unwrap()
    };
    let compiler2 = GraphCardCompiler::new(Box::new(graph2), Some(repo.path()));
    let normal = compiler2.symbol_card(sym_id, Budget::Normal).unwrap();
    assert!(
        normal.source_body.is_none(),
        "normal budget must not include source body"
    );

    let graph3 = {
        let graph_dir = repo.path().join(".synrepo/graph");
        SqliteGraphStore::open(&graph_dir).unwrap()
    };
    let compiler3 = GraphCardCompiler::new(Box::new(graph3), Some(repo.path()));
    let deep = compiler3.symbol_card(sym_id, Budget::Deep).unwrap();
    assert!(
        deep.source_body.is_some(),
        "deep budget must include source body"
    );
    let body = deep.source_body.unwrap();
    assert!(
        body.contains("documented"),
        "source body must contain function text"
    );
}

#[test]
fn file_card_includes_imports_edges() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(
        repo.path().join("src/utils.ts"),
        "export function helper() {}\n",
    )
    .unwrap();
    fs::write(
        repo.path().join("src/main.ts"),
        "import { helper } from './utils';\nhelper();\n",
    )
    .unwrap();

    let graph = bootstrap(&repo);
    let main_id = graph.file_by_path("src/main.ts").unwrap().unwrap().id;
    let utils_id = graph.file_by_path("src/utils.ts").unwrap().unwrap().id;
    let compiler = make_compiler(graph, &repo);

    let card = compiler.file_card(main_id, Budget::Normal).unwrap();
    assert!(
        card.imports.iter().any(|r| r.id == utils_id),
        "main.ts card must list utils.ts as an import"
    );

    let graph2 = {
        let graph_dir = repo.path().join(".synrepo/graph");
        SqliteGraphStore::open(&graph_dir).unwrap()
    };
    let compiler2 = GraphCardCompiler::new(Box::new(graph2), Some(repo.path()));
    let utils_card = compiler2.file_card(utils_id, Budget::Normal).unwrap();
    assert!(
        utils_card.imported_by.iter().any(|r| r.id == main_id),
        "utils.ts card must list main.ts in imported_by"
    );
}

#[test]
fn symbol_card_snapshots_with_signature_and_doc_comment() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(
        repo.path().join("src/lib.rs"),
        "/// Add two integers together.\npub fn add(a: i32, b: i32) -> i32 { a + b }\n",
    )
    .unwrap();

    let graph = bootstrap(&repo);
    let file = graph.file_by_path("src/lib.rs").unwrap().unwrap();
    let sym_edge = graph
        .outbound(NodeId::File(file.id), Some(EdgeKind::Defines))
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    let sym_id = match sym_edge.to {
        NodeId::Symbol(id) => id,
        _ => panic!("expected symbol"),
    };
    let compiler = make_compiler(graph, &repo);

    // Snapshot all three budget tiers so regressions are visible.
    let tiny =
        serde_json::to_string_pretty(&compiler.symbol_card(sym_id, Budget::Tiny).unwrap()).unwrap();
    assert_snapshot!("symbol_card_tiny", tiny);

    let normal =
        serde_json::to_string_pretty(&compiler.symbol_card(sym_id, Budget::Normal).unwrap())
            .unwrap();
    assert_snapshot!("symbol_card_normal", normal);

    let deep =
        serde_json::to_string_pretty(&compiler.symbol_card(sym_id, Budget::Deep).unwrap()).unwrap();
    assert_snapshot!("symbol_card_deep", deep);
}

fn fresh_symbol_fixture() -> (
    tempfile::TempDir,
    SqliteGraphStore,
    crate::core::ids::SymbolNodeId,
) {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(
        repo.path().join("src/lib.rs"),
        "/// Docs.\npub fn annotated() -> u32 { 7 }\n",
    )
    .unwrap();

    let graph = bootstrap(&repo);
    let file = graph.file_by_path("src/lib.rs").unwrap().unwrap();
    let sym_edge = graph
        .outbound(NodeId::File(file.id), Some(EdgeKind::Defines))
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    let sym_id = match sym_edge.to {
        NodeId::Symbol(id) => id,
        _ => panic!("expected symbol"),
    };
    (repo, graph, sym_id)
}

fn current_content_hash(graph: &SqliteGraphStore, path: &str) -> String {
    graph.file_by_path(path).unwrap().unwrap().content_hash
}

fn make_overlay_store(repo: &tempfile::TempDir) -> Arc<Mutex<dyn OverlayStore>> {
    let overlay_dir = repo.path().join(".synrepo/overlay");
    let store = SqliteOverlayStore::open(&overlay_dir).unwrap();
    Arc::new(Mutex::new(store))
}

#[test]
fn symbol_card_deep_with_fresh_commentary_reports_fresh_state() {
    let (repo, graph, sym_id) = fresh_symbol_fixture();
    let hash = current_content_hash(&graph, "src/lib.rs");
    let overlay = make_overlay_store(&repo);

    overlay
        .lock()
        .insert_commentary(CommentaryEntry {
            node_id: NodeId::Symbol(sym_id),
            text: "Annotated function.".to_string(),
            provenance: CommentaryProvenance {
                source_content_hash: hash.clone(),
                pass_id: "test".to_string(),
                model_identity: "claude-fixture".to_string(),
                generated_at: OffsetDateTime::now_utc(),
            },
        })
        .unwrap();

    let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()))
        .with_overlay(Some(overlay), None);
    let card = compiler.symbol_card(sym_id, Budget::Deep).unwrap();

    assert_eq!(card.commentary_state.as_deref(), Some("fresh"));
    let commentary = card.overlay_commentary.expect("commentary present");
    assert_eq!(commentary.text, "Annotated function.");
    assert_eq!(commentary.freshness, Freshness::Fresh);
    assert_eq!(commentary.source_store, SourceStore::Overlay);
}

#[test]
fn symbol_card_deep_with_stale_commentary_reports_stale_state() {
    let (repo, graph, sym_id) = fresh_symbol_fixture();
    let overlay = make_overlay_store(&repo);

    overlay
        .lock()
        .insert_commentary(CommentaryEntry {
            node_id: NodeId::Symbol(sym_id),
            text: "Stale annotation.".to_string(),
            provenance: CommentaryProvenance {
                source_content_hash: "outdated-hash".to_string(),
                pass_id: "test".to_string(),
                model_identity: "claude-fixture".to_string(),
                generated_at: OffsetDateTime::now_utc(),
            },
        })
        .unwrap();

    let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()))
        .with_overlay(Some(overlay), None);
    let card = compiler.symbol_card(sym_id, Budget::Deep).unwrap();

    assert_eq!(card.commentary_state.as_deref(), Some("stale"));
    let commentary = card
        .overlay_commentary
        .expect("stale commentary still returned");
    assert_eq!(commentary.freshness, Freshness::Stale);
}

#[test]
fn symbol_card_deep_missing_commentary_with_noop_generator() {
    let (repo, graph, sym_id) = fresh_symbol_fixture();
    let overlay = make_overlay_store(&repo);
    let generator: Arc<dyn CommentaryGenerator> = Arc::new(NoOpGenerator);

    let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()))
        .with_overlay(Some(overlay), Some(generator));
    let card = compiler.symbol_card(sym_id, Budget::Deep).unwrap();

    assert_eq!(card.commentary_state.as_deref(), Some("missing"));
    assert!(card.overlay_commentary.is_none());
}

#[test]
fn symbol_card_tiny_and_normal_report_budget_withheld() {
    let (repo, graph, sym_id) = fresh_symbol_fixture();
    let overlay = make_overlay_store(&repo);

    let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()))
        .with_overlay(Some(overlay), None);

    for budget in [Budget::Tiny, Budget::Normal] {
        let card = compiler.symbol_card(sym_id, budget).unwrap();
        assert_eq!(
            card.commentary_state.as_deref(),
            Some("budget_withheld"),
            "budget {budget:?} must report budget_withheld"
        );
        assert!(card.overlay_commentary.is_none());
    }
}

#[test]
fn symbol_card_deep_with_generator_persists_new_entry() {
    use crate::overlay::CommentaryEntry;

    struct AlwaysGenerate;
    impl CommentaryGenerator for AlwaysGenerate {
        fn generate(&self, node: NodeId, _context: &str) -> crate::Result<Option<CommentaryEntry>> {
            Ok(Some(CommentaryEntry {
                node_id: node,
                text: "Freshly generated.".to_string(),
                provenance: CommentaryProvenance {
                    source_content_hash: String::new(),
                    pass_id: "test".to_string(),
                    model_identity: "fixture".to_string(),
                    generated_at: OffsetDateTime::now_utc(),
                },
            }))
        }
    }

    let (repo, graph, sym_id) = fresh_symbol_fixture();
    let overlay = make_overlay_store(&repo);
    let generator: Arc<dyn CommentaryGenerator> = Arc::new(AlwaysGenerate);

    let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()))
        .with_overlay(Some(overlay.clone()), Some(generator));
    let card = compiler.symbol_card(sym_id, Budget::Deep).unwrap();

    assert_eq!(card.commentary_state.as_deref(), Some("fresh"));
    let commentary = card
        .overlay_commentary
        .expect("generated commentary present");
    assert_eq!(commentary.text, "Freshly generated.");

    // Side effect: entry persisted to the overlay with the current hash.
    let persisted = overlay
        .lock()
        .commentary_for(NodeId::Symbol(sym_id))
        .unwrap()
        .expect("entry persisted");
    assert_eq!(persisted.text, "Freshly generated.");
    assert!(!persisted.provenance.source_content_hash.is_empty());
}

fn sample_proposed_link(
    from: NodeId,
    to: NodeId,
    from_hash: &str,
    to_hash: &str,
    tier: crate::overlay::ConfidenceTier,
) -> crate::overlay::OverlayLink {
    crate::overlay::OverlayLink {
        from,
        to,
        kind: crate::overlay::OverlayEdgeKind::References,
        epistemic: crate::overlay::OverlayEpistemic::MachineAuthoredHighConf,
        source_spans: vec![crate::overlay::CitedSpan {
            artifact: from,
            normalized_text: "span".into(),
            verified_at_offset: 0,
            lcs_ratio: 1.0,
        }],
        target_spans: vec![crate::overlay::CitedSpan {
            artifact: to,
            normalized_text: "span".into(),
            verified_at_offset: 0,
            lcs_ratio: 1.0,
        }],
        from_content_hash: from_hash.into(),
        to_content_hash: to_hash.into(),
        confidence_score: match tier {
            crate::overlay::ConfidenceTier::High => 0.9,
            crate::overlay::ConfidenceTier::ReviewQueue => 0.7,
            crate::overlay::ConfidenceTier::BelowThreshold => 0.4,
        },
        confidence_tier: tier,
        rationale: None,
        provenance: crate::overlay::CrossLinkProvenance {
            pass_id: "test".into(),
            model_identity: "test".into(),
            generated_at: OffsetDateTime::now_utc(),
        },
    }
}

#[test]
fn symbol_card_deep_with_fresh_high_tier_link() {
    let (repo, graph, sym_id) = fresh_symbol_fixture();
    let hash = current_content_hash(&graph, "src/lib.rs");
    let overlay = make_overlay_store(&repo);

    let from_id = NodeId::Symbol(sym_id);
    let file_id = graph.file_by_path("src/lib.rs").unwrap().unwrap().id;
    let to_id = NodeId::File(file_id);
    let link = sample_proposed_link(
        from_id,
        to_id,
        &hash,
        &hash,
        crate::overlay::ConfidenceTier::High,
    );
    overlay.lock().insert_link(link).unwrap();

    let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()))
        .with_overlay(Some(overlay), None);
    let card = compiler.symbol_card(sym_id, Budget::Deep).unwrap();

    assert_eq!(card.links_state.as_deref(), Some("present"));
    let links = card
        .proposed_links
        .as_ref()
        .expect("proposed links present");
    assert_eq!(links.len(), 1);
    assert_eq!(
        links[0].freshness,
        crate::overlay::CrossLinkFreshness::Fresh
    );

    // Snapshot it
    let json = serde_json::to_string_pretty(&card).unwrap();
    assert_snapshot!("symbol_card_deep_with_proposed_links", json);
}

#[test]
fn symbol_card_normal_reports_budget_withheld_for_links() {
    let (repo, graph, sym_id) = fresh_symbol_fixture();
    let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()));

    let card = compiler.symbol_card(sym_id, Budget::Normal).unwrap();
    assert_eq!(card.links_state.as_deref(), Some("budget_withheld"));
    assert!(card.proposed_links.is_none());
}

#[test]
fn symbol_card_deep_missing_links_state() {
    let (repo, graph, sym_id) = fresh_symbol_fixture();
    let overlay = make_overlay_store(&repo);

    let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()))
        .with_overlay(Some(overlay), None);
    let card = compiler.symbol_card(sym_id, Budget::Deep).unwrap();

    assert_eq!(card.links_state.as_deref(), Some("missing"));
    assert!(card.proposed_links.is_none());
}

#[test]
fn symbol_card_deep_filters_below_threshold_links() {
    let (repo, graph, sym_id) = fresh_symbol_fixture();
    let hash = current_content_hash(&graph, "src/lib.rs");
    let overlay = make_overlay_store(&repo);

    let from_id = NodeId::Symbol(sym_id);
    let file_id = graph.file_by_path("src/lib.rs").unwrap().unwrap().id;
    let to_id = NodeId::File(file_id);
    let link = sample_proposed_link(
        from_id,
        to_id,
        &hash,
        &hash,
        crate::overlay::ConfidenceTier::BelowThreshold,
    );
    overlay.lock().insert_link(link).unwrap();

    let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()))
        .with_overlay(Some(overlay), None);
    let card = compiler.symbol_card(sym_id, Budget::Deep).unwrap();

    // Since the only link is BelowThreshold, it's filtered out, making the state "missing"
    assert_eq!(card.links_state.as_deref(), Some("missing"));
    assert!(card.proposed_links.is_none() || card.proposed_links.unwrap().is_empty());
}

#[test]
fn symbol_card_deep_stale_link_preservation() {
    let (repo, graph, sym_id) = fresh_symbol_fixture();
    let _hash = current_content_hash(&graph, "src/lib.rs");
    let overlay = make_overlay_store(&repo);

    let from_id = NodeId::Symbol(sym_id);
    let file_id = graph.file_by_path("src/lib.rs").unwrap().unwrap().id;
    let to_id = NodeId::File(file_id);
    // Link has out-of-date hash
    let link = sample_proposed_link(
        from_id,
        to_id,
        "old-hash",
        "old-hash",
        crate::overlay::ConfidenceTier::High,
    );
    overlay.lock().insert_link(link).unwrap();

    let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()))
        .with_overlay(Some(overlay), None);
    let card = compiler.symbol_card(sym_id, Budget::Deep).unwrap();

    assert_eq!(card.links_state.as_deref(), Some("present"));
    let links = card.proposed_links.as_ref().unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(
        links[0].freshness,
        crate::overlay::CrossLinkFreshness::Stale
    );
}

// 7.5: entry_point_card returns empty list (no panic) when no files are indexed

#[test]
fn entry_point_card_empty_repo_returns_no_panic() {
    let repo = tempdir().unwrap();
    // Bootstrap produces an empty graph (no source files to index).
    let graph = bootstrap(&repo);
    let compiler = make_compiler(graph, &repo);

    let card = compiler
        .entry_point_card(None, Budget::Tiny)
        .expect("entry_point_card must not error on empty graph");
    assert!(
        card.entry_points.is_empty(),
        "empty graph must produce empty entry_points list"
    );
    assert_eq!(card.source_store, SourceStore::Graph);
}
