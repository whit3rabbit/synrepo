pub(super) mod accept;
pub(super) mod accept_validation;
pub(super) mod commit_fault_injection;
pub(super) mod findings;
pub(super) mod list;
pub(super) mod locks;
pub(super) mod reject;
pub(super) mod review;

pub(super) use super::super::commands;
pub(super) use super::support;

use synrepo::config::Config;
use synrepo::core::ids::NodeId;
use synrepo::overlay::{
    CitedSpan, ConfidenceTier, CrossLinkProvenance, OverlayEdgeKind, OverlayEpistemic, OverlayLink,
};
use synrepo::store::overlay::SqliteOverlayStore;
use tempfile::tempdir;
use time::OffsetDateTime;

use support::seed_graph;

/// Bring up a curated-mode repo with an overlay store opened, returning the
/// from/to node ids the `links_accept_*` tests share.
pub(super) fn setup_curated_link_env() -> (tempfile::TempDir, SqliteOverlayStore, NodeId, NodeId) {
    let repo = tempdir().unwrap();
    let ids = seed_graph(repo.path());
    std::fs::write(
        Config::synrepo_dir(repo.path()).join("config.toml"),
        "mode = \"curated\"\n",
    )
    .unwrap();

    let synrepo_dir = Config::synrepo_dir(repo.path());
    let overlay = SqliteOverlayStore::open(&synrepo_dir.join("overlay")).unwrap();
    let from = NodeId::Concept(ids.concept_id);
    let to = NodeId::Symbol(ids.symbol_id);
    (repo, overlay, from, to)
}

pub(super) fn sample_link(from: NodeId, to: NodeId) -> OverlayLink {
    OverlayLink {
        from,
        to,
        kind: OverlayEdgeKind::References,
        epistemic: OverlayEpistemic::MachineAuthoredHighConf,
        source_spans: vec![CitedSpan {
            artifact: from,
            normalized_text: "source".into(),
            verified_at_offset: 0,
            lcs_ratio: 1.0,
        }],
        target_spans: vec![CitedSpan {
            artifact: to,
            normalized_text: "target".into(),
            verified_at_offset: 0,
            lcs_ratio: 1.0,
        }],
        from_content_hash: "h1".into(),
        to_content_hash: "h2".into(),
        confidence_score: 0.95,
        confidence_tier: ConfidenceTier::High,
        rationale: Some("Test rationale".into()),
        provenance: CrossLinkProvenance {
            pass_id: "test-pass".into(),
            model_identity: "test-model".into(),
            generated_at: OffsetDateTime::now_utc(),
        },
    }
}

pub(super) fn write_curated_mode(repo: &std::path::Path) {
    std::fs::write(
        Config::synrepo_dir(repo).join("config.toml"),
        "mode = \"curated\"\n",
    )
    .unwrap();
}

/// Open-for-create so `links_accept` malformed-input tests reach the parser
/// branch instead of bailing on `open_existing`.
pub(super) fn ensure_overlay_initialized(repo: &std::path::Path) {
    let overlay_dir = Config::synrepo_dir(repo).join("overlay");
    let _ = SqliteOverlayStore::open(&overlay_dir).unwrap();
}
