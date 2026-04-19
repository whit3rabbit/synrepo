//! Proposed cross-link resolution tests at Deep budget: present / missing /
//! budget_withheld / below-threshold filtering / stale preservation.

use super::super::{Budget, GraphCardCompiler};
use super::fixtures::{
    current_content_hash, fresh_symbol_fixture, make_overlay_store, sample_proposed_link,
};
use crate::{core::ids::NodeId, surface::card::CardCompiler};
use insta::assert_snapshot;

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

    let compiler =
        GraphCardCompiler::new(Box::new(graph), Some(repo.path())).with_overlay(Some(overlay));
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

    let compiler =
        GraphCardCompiler::new(Box::new(graph), Some(repo.path())).with_overlay(Some(overlay));
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

    let compiler =
        GraphCardCompiler::new(Box::new(graph), Some(repo.path())).with_overlay(Some(overlay));
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

    let compiler =
        GraphCardCompiler::new(Box::new(graph), Some(repo.path())).with_overlay(Some(overlay));
    let card = compiler.symbol_card(sym_id, Budget::Deep).unwrap();

    assert_eq!(card.links_state.as_deref(), Some("present"));
    let links = card.proposed_links.as_ref().unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(
        links[0].freshness,
        crate::overlay::CrossLinkFreshness::Stale
    );
}
