//! Overlay commentary resolution tests: fresh / stale / missing / budget_withheld,
//! explicit refresh persistence, and read-only guarantees.

use super::super::{Budget, GraphCardCompiler, SourceStore};
use super::fixtures::{current_content_hash, fresh_symbol_fixture, make_overlay_store};
use crate::{
    core::ids::NodeId,
    overlay::{CommentaryEntry, CommentaryProvenance},
    pipeline::explain::CommentaryGenerator,
    surface::card::{CardCompiler, Freshness},
};
use time::OffsetDateTime;

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

    let compiler =
        GraphCardCompiler::new(Box::new(graph), Some(repo.path())).with_overlay(Some(overlay));
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

    let compiler =
        GraphCardCompiler::new(Box::new(graph), Some(repo.path())).with_overlay(Some(overlay));
    let card = compiler.symbol_card(sym_id, Budget::Deep).unwrap();

    assert_eq!(card.commentary_state.as_deref(), Some("stale"));
    let commentary = card
        .overlay_commentary
        .expect("stale commentary still returned");
    assert_eq!(commentary.freshness, Freshness::Stale);
}

#[test]
fn symbol_card_deep_missing_commentary_reports_missing() {
    let (repo, graph, sym_id) = fresh_symbol_fixture();
    let overlay = make_overlay_store(&repo);

    let compiler =
        GraphCardCompiler::new(Box::new(graph), Some(repo.path())).with_overlay(Some(overlay));
    let card = compiler.symbol_card(sym_id, Budget::Deep).unwrap();

    assert_eq!(card.commentary_state.as_deref(), Some("missing"));
    assert!(card.overlay_commentary.is_none());
}

#[test]
fn symbol_card_tiny_and_normal_report_budget_withheld() {
    let (repo, graph, sym_id) = fresh_symbol_fixture();
    let overlay = make_overlay_store(&repo);

    let compiler =
        GraphCardCompiler::new(Box::new(graph), Some(repo.path())).with_overlay(Some(overlay));

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
fn refresh_commentary_persists_new_entry() {
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
    let generator = AlwaysGenerate;

    let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()))
        .with_overlay(Some(overlay.clone()));

    // Verify it's missing initially.
    let card = compiler.symbol_card(sym_id, Budget::Deep).unwrap();
    assert_eq!(card.commentary_state.as_deref(), Some("missing"));

    // Explicitly refresh.
    let text = compiler
        .refresh_commentary(NodeId::Symbol(sym_id), &generator)
        .unwrap();
    assert_eq!(text.as_deref(), Some("Freshly generated."));

    let synrepo_dir = repo.path().join(".synrepo");
    let doc_path = synrepo_dir
        .join("explain-docs")
        .join("symbols")
        .join(format!("{}.md", NodeId::Symbol(sym_id)));
    assert!(
        doc_path.exists(),
        "expected explained doc at {}",
        doc_path.display()
    );
    let hits = crate::pipeline::explain::docs::search_commentary_index(
        &synrepo_dir,
        "Freshly generated.",
        10,
    )
    .unwrap();
    assert_eq!(hits.len(), 1);

    // Verify it's now fresh in the card.
    let card = compiler.symbol_card(sym_id, Budget::Deep).unwrap();
    assert_eq!(card.commentary_state.as_deref(), Some("fresh"));
    assert_eq!(card.overlay_commentary.unwrap().text, "Freshly generated.");
}

#[test]
fn symbol_card_read_is_strictly_readonly() {
    let (repo, graph, sym_id) = fresh_symbol_fixture();
    let overlay = make_overlay_store(&repo);

    let compiler =
        GraphCardCompiler::new(Box::new(graph), Some(repo.path())).with_overlay(Some(overlay));

    // This must not panic because the compiler doesn't even have a generator anymore,
    // and symbol_card is now strictly read-only.
    let card = compiler.symbol_card(sym_id, Budget::Deep).unwrap();
    assert_eq!(card.commentary_state.as_deref(), Some("missing"));
}
