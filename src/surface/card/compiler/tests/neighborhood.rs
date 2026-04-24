//! Neighborhood resolution tests (synrepo-minimum-context).

use super::super::neighborhood::{resolve_neighborhood, CoChangeState};
use super::super::Budget;
use super::fixtures::{multi_file_fixture, neighborhood_fixture};

#[test]
fn neighborhood_tiny_returns_focal_card_with_edge_counts() {
    let (_repo, compiler, _file_id, sym_id) = neighborhood_fixture();

    let resp = resolve_neighborhood(&compiler, &sym_id.to_string(), Budget::Tiny).unwrap();

    assert_eq!(resp.budget, "tiny");
    assert!(resp.focal_card.is_object());
    assert!(
        resp.neighbors.is_none(),
        "tiny must not include neighbor cards"
    );
    assert!(
        resp.neighbor_summaries.is_none(),
        "tiny must not include summaries"
    );
    assert!(resp.decision_cards.is_none());
    assert!(
        resp.co_change_partners.is_none(),
        "tiny must not include co-change details"
    );
    // Edge counts are always present (values validated by serialization test).
}

#[test]
fn neighborhood_normal_returns_neighbor_summaries() {
    let (_repo, compiler, _file_id, sym_id) = neighborhood_fixture();

    let resp = resolve_neighborhood(&compiler, &sym_id.to_string(), Budget::Normal).unwrap();

    assert_eq!(resp.budget, "normal");
    assert!(
        resp.neighbors.is_none(),
        "normal must not include full neighbor cards"
    );
    // neighbor_summaries may be Some(empty) or None depending on edges
    if let Some(summaries) = &resp.neighbor_summaries {
        for s in summaries {
            assert!(!s.node_id.is_empty());
            assert!(!s.kind.is_empty());
            assert!(!s.edge_type.is_empty());
        }
    }
}

#[test]
fn neighborhood_deep_returns_full_neighbor_cards() {
    let (_repo, compiler, _file_id, sym_id) = neighborhood_fixture();

    let resp = resolve_neighborhood(&compiler, &sym_id.to_string(), Budget::Deep).unwrap();

    assert_eq!(resp.budget, "deep");
    assert!(
        resp.neighbor_summaries.is_none(),
        "deep must not include summaries"
    );
    if let Some(neighbors) = &resp.neighbors {
        for card in neighbors {
            assert!(card.is_object());
            // Verify overlay fields are stripped from neighbor cards
            let obj = card.as_object().unwrap();
            assert!(!obj.contains_key("overlay_commentary"));
            assert!(!obj.contains_key("proposed_links"));
        }
    }
}

#[test]
fn neighborhood_unresolved_target_returns_error() {
    let (_repo, compiler, _file_id, _sym_id) = neighborhood_fixture();

    let result = resolve_neighborhood(&compiler, "nonexistent_xyz", Budget::Normal);
    let err = result.expect_err("must error for unresolved target");
    let msg = err.to_string();
    assert!(
        msg.contains("target not found: nonexistent_xyz"),
        "error must include the target string, got: {msg}"
    );
}

#[test]
fn neighborhood_missing_git_data_returns_co_change_missing() {
    let (_repo, compiler, _file_id, sym_id) = neighborhood_fixture();

    // Without config, git intelligence is unavailable.
    let resp = resolve_neighborhood(&compiler, &sym_id.to_string(), Budget::Normal).unwrap();

    assert_eq!(resp.co_change_state, CoChangeState::Missing);
    if let Some(partners) = &resp.co_change_partners {
        assert!(partners.is_empty());
    }
}

#[test]
fn neighborhood_file_target_resolves() {
    let (_repo, compiler, _main_id) = multi_file_fixture();

    let resp = resolve_neighborhood(&compiler, "src/main.ts", Budget::Normal).unwrap();

    assert!(resp.focal_card.is_object());
    assert_eq!(resp.budget, "normal");
}

#[test]
fn neighborhood_overlay_fields_stripped_from_focal_card() {
    let (_repo, compiler, _file_id, sym_id) = neighborhood_fixture();

    let resp = resolve_neighborhood(&compiler, &sym_id.to_string(), Budget::Deep).unwrap();

    let obj = resp.focal_card.as_object().unwrap();
    assert!(!obj.contains_key("overlay_commentary"));
    assert!(!obj.contains_key("proposed_links"));
    assert!(!obj.contains_key("commentary_state"));
    assert!(!obj.contains_key("links_state"));
}

#[test]
fn neighborhood_focal_card_retains_escalation_accounting() {
    // The focal card in a minimum-context response must expose the
    // accounting metadata agents need to decide whether to escalate from
    // a bounded card to a deeper card or a full-file read.
    let (_repo, compiler, _file_id, sym_id) = neighborhood_fixture();

    let resp = resolve_neighborhood(&compiler, &sym_id.to_string(), Budget::Tiny).unwrap();
    let accounting = resp
        .focal_card
        .pointer("/context_accounting")
        .expect("focal_card must expose context_accounting for escalation decisions");
    let obj = accounting
        .as_object()
        .expect("context_accounting must be an object");

    for key in [
        "budget_tier",
        "token_estimate",
        "raw_file_token_estimate",
        "estimated_savings_ratio",
        "source_hashes",
        "stale",
        "truncation_applied",
    ] {
        assert!(
            obj.contains_key(key),
            "context_accounting must retain `{key}` so agents can decide when to escalate"
        );
    }
}
