//! Cross-link insert/retrieve/prune tests.

use crate::core::ids::{ConceptNodeId, NodeId, SymbolNodeId};
use crate::overlay::{
    CitedSpan, ConfidenceTier, CrossLinkProvenance, OverlayEdgeKind, OverlayEpistemic, OverlayLink,
    OverlayStore,
};
use crate::store::overlay::SqliteOverlayStore;
use tempfile::tempdir;

pub fn sample_link(from: NodeId, to: NodeId, from_hash: &str, to_hash: &str) -> OverlayLink {
    OverlayLink {
        from,
        to,
        kind: OverlayEdgeKind::References,
        epistemic: OverlayEpistemic::MachineAuthoredHighConf,
        source_spans: vec![CitedSpan {
            artifact: from,
            normalized_text: "authenticate".into(),
            verified_at_offset: 12,
            lcs_ratio: 0.98,
        }],
        target_spans: vec![CitedSpan {
            artifact: to,
            normalized_text: "fn authenticate".into(),
            verified_at_offset: 3,
            lcs_ratio: 1.0,
        }],
        from_content_hash: from_hash.into(),
        to_content_hash: to_hash.into(),
        confidence_score: 0.91,
        confidence_tier: ConfidenceTier::High,
        rationale: Some("Prose names the symbol; matches qualified name.".into()),
        provenance: CrossLinkProvenance {
            pass_id: "cross-link-v1".into(),
            model_identity: "claude-sonnet-4-6".into(),
            generated_at: time::OffsetDateTime::from_unix_timestamp(1_712_000_000).unwrap(),
        },
    }
}

fn sample_commentary_entry(node: NodeId, hash: &str) -> crate::overlay::CommentaryEntry {
    crate::overlay::CommentaryEntry {
        node_id: node,
        text: "Sample text".to_string(),
        provenance: crate::overlay::CommentaryProvenance {
            source_content_hash: hash.to_string(),
            pass_id: "commentary-v1".to_string(),
            model_identity: "claude-sonnet-4-6".to_string(),
            generated_at: time::OffsetDateTime::from_unix_timestamp(1_712_000_000).unwrap(),
        },
    }
}

#[test]
fn cross_link_insert_and_retrieve_by_either_endpoint() {
    let dir = tempdir().unwrap();
    let mut store = SqliteOverlayStore::open(dir.path()).unwrap();

    let from = NodeId::Concept(ConceptNodeId(1));
    let to = NodeId::Symbol(SymbolNodeId(2));
    let link = sample_link(from, to, "h-from", "h-to");
    store.insert_link(link.clone()).unwrap();

    let by_from = store.links_for(from).unwrap();
    assert_eq!(by_from.len(), 1);
    assert_eq!(by_from[0].from, from);
    assert_eq!(by_from[0].to, to);
    assert_eq!(by_from[0].confidence_tier, ConfidenceTier::High);
    assert_eq!(by_from[0].from_content_hash, "h-from");
    assert_eq!(by_from[0].to_content_hash, "h-to");
    assert_eq!(by_from[0].source_spans.len(), 1);
    assert_eq!(by_from[0].target_spans.len(), 1);
    assert_eq!(by_from[0].provenance.pass_id, "cross-link-v1");

    let by_to = store.links_for(to).unwrap();
    assert_eq!(by_to.len(), 1);
}

#[test]
fn cross_link_insert_rejects_missing_provenance() {
    let dir = tempdir().unwrap();
    let mut store = SqliteOverlayStore::open(dir.path()).unwrap();

    let from = NodeId::Concept(ConceptNodeId(1));
    let to = NodeId::Symbol(SymbolNodeId(2));
    let mut link = sample_link(from, to, "h-from", "h-to");
    link.provenance.pass_id = String::new();

    let err = store.insert_link(link).unwrap_err();
    assert!(
        err.to_string().contains("provenance"),
        "expected provenance error, got: {err}"
    );
}

#[test]
fn cross_link_insert_rejects_empty_spans() {
    let dir = tempdir().unwrap();
    let mut store = SqliteOverlayStore::open(dir.path()).unwrap();

    let from = NodeId::Concept(ConceptNodeId(1));
    let to = NodeId::Symbol(SymbolNodeId(2));
    let mut link = sample_link(from, to, "h-from", "h-to");
    link.source_spans.clear();

    assert!(store.insert_link(link).is_err());
}

#[test]
fn cross_link_upsert_records_regeneration_audit() {
    let dir = tempdir().unwrap();
    let mut store = SqliteOverlayStore::open(dir.path()).unwrap();

    let from = NodeId::Concept(ConceptNodeId(1));
    let to = NodeId::Symbol(SymbolNodeId(2));
    let mut link = sample_link(from, to, "h-from", "h-to");
    store.insert_link(link.clone()).unwrap();

    link.from_content_hash = "h-from-v2".into();
    store.insert_link(link.clone()).unwrap();

    assert_eq!(store.cross_link_count().unwrap(), 1);
    let audit = store
        .cross_link_audit_events(&from.to_string(), &to.to_string(), "references")
        .unwrap();
    assert_eq!(audit.len(), 2);
    assert_eq!(audit[0].event_kind, "generated");
    assert_eq!(audit[1].event_kind, "regenerated");
}

#[test]
fn cross_link_prune_orphans_writes_audit_and_removes_row() {
    let dir = tempdir().unwrap();
    let mut store = SqliteOverlayStore::open(dir.path()).unwrap();

    let keep_from = NodeId::Concept(ConceptNodeId(1));
    let keep_to = NodeId::Symbol(SymbolNodeId(2));
    let drop_from = NodeId::Concept(ConceptNodeId(3));
    let drop_to = NodeId::Symbol(SymbolNodeId(4));

    store
        .insert_link(sample_link(keep_from, keep_to, "hk-from", "hk-to"))
        .unwrap();
    store
        .insert_link(sample_link(drop_from, drop_to, "hd-from", "hd-to"))
        .unwrap();

    // Only the first pair's endpoints remain live.
    let live = [keep_from, keep_to];
    let removed = store.prune_orphans(&live).unwrap();
    assert_eq!(removed, 1);
    assert_eq!(store.cross_link_count().unwrap(), 1);

    let audit = store
        .cross_link_audit_events(&drop_from.to_string(), &drop_to.to_string(), "references")
        .unwrap();
    let pruned = audit
        .iter()
        .find(|r| r.event_kind == "pruned")
        .expect("expected a `pruned` audit row");
    assert_eq!(pruned.reason.as_deref(), Some("source_deleted"));
}

#[test]
fn cross_link_schema_isolated_from_commentary() {
    // Commentary writes and cross-link writes hit different tables; neither
    // sees the other's rows.
    let dir = tempdir().unwrap();
    let mut store = SqliteOverlayStore::open(dir.path()).unwrap();

    let sym = NodeId::Symbol(SymbolNodeId(99));
    store
        .insert_commentary(sample_commentary_entry(sym, "h1"))
        .unwrap();

    let from = NodeId::Concept(ConceptNodeId(1));
    let to = NodeId::Symbol(SymbolNodeId(2));
    store
        .insert_link(sample_link(from, to, "h-from", "h-to"))
        .unwrap();

    assert_eq!(store.commentary_count().unwrap(), 1);
    assert_eq!(store.cross_link_count().unwrap(), 1);

    let conn = store.conn.lock();
    let cross_in_commentary: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM commentary WHERE node_id = ?1",
            [from.to_string()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(cross_in_commentary, 0);
    let comm_in_cross: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM cross_links WHERE from_node = ?1",
            [sym.to_string()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(comm_in_cross, 0);
}
