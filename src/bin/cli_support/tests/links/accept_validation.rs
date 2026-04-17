use synrepo::core::ids::NodeId;
use synrepo::overlay::OverlayEdgeKind;
use synrepo::store::overlay::format_candidate_id;
use tempfile::tempdir;

use super::support::seed_graph;
use super::{commands, ensure_overlay_initialized, write_curated_mode};

#[test]
fn links_accept_rejects_malformed_candidate_id() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());
    write_curated_mode(repo.path());
    ensure_overlay_initialized(repo.path());

    // Two `::` separators only -> 2 parts; parser requires 3 or 4.
    let err = commands::links_accept(repo.path(), "only::two", None).unwrap_err();
    assert!(
        err.to_string().contains("Invalid candidate ID format"),
        "expected format error, got: {err}"
    );
}

#[test]
fn links_accept_rejects_invalid_from_node_id() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());
    write_curated_mode(repo.path());
    ensure_overlay_initialized(repo.path());

    let err = commands::links_accept(
        repo.path(),
        "bogus_from::sym_0000000000000002::references::pass",
        None,
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("Invalid from_node"),
        "expected `Invalid from_node`, got: {err}"
    );
}

#[test]
fn links_accept_rejects_invalid_edge_kind() {
    let repo = tempdir().unwrap();
    seed_graph(repo.path());
    write_curated_mode(repo.path());
    ensure_overlay_initialized(repo.path());

    let err = commands::links_accept(
        repo.path(),
        "concept_0000000000000001::sym_0000000000000002::not_a_kind::pass",
        None,
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("Invalid edge kind"),
        "expected `Invalid edge kind`, got: {err}"
    );
}

#[test]
fn links_accept_missing_candidate_returns_error() {
    let repo = tempdir().unwrap();
    let ids = seed_graph(repo.path());
    write_curated_mode(repo.path());
    ensure_overlay_initialized(repo.path());

    let from = NodeId::Concept(ids.concept_id);
    let to = NodeId::Symbol(ids.symbol_id);
    // Well-formed candidate id, but no overlay row exists for this triple.
    let candidate_id = format_candidate_id(from, to, OverlayEdgeKind::References, "fresh-pass");

    let err = commands::links_accept(repo.path(), &candidate_id, None).unwrap_err();
    assert!(
        err.to_string().contains("Candidate not found"),
        "expected `Candidate not found`, got: {err}"
    );
}
