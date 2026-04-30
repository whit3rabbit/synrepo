//! Freshness derivation tests.

use crate::core::ids::{NodeId, SymbolNodeId};
use crate::overlay::{CommentaryEntry, CommentaryProvenance, FreshnessState};
use crate::store::overlay::derive_freshness;

fn sample_entry(node_id: NodeId, hash: &str) -> CommentaryEntry {
    CommentaryEntry {
        node_id,
        text: "Sample commentary.".to_string(),
        provenance: CommentaryProvenance {
            source_content_hash: hash.to_string(),
            pass_id: "commentary-v4".to_string(),
            model_identity: "claude-sonnet-4-6".to_string(),
            generated_at: time::OffsetDateTime::from_unix_timestamp(1_712_000_000).unwrap(),
        },
    }
}

#[test]
fn derive_freshness_fresh_on_hash_match() {
    let node = NodeId::Symbol(SymbolNodeId(1));
    let entry = sample_entry(node, "hash-fresh");
    assert_eq!(
        derive_freshness(&entry, "hash-fresh"),
        FreshnessState::Fresh
    );
}

#[test]
fn derive_freshness_stale_on_hash_mismatch() {
    let node = NodeId::Symbol(SymbolNodeId(1));
    let entry = sample_entry(node, "hash-old");
    assert_eq!(derive_freshness(&entry, "hash-new"), FreshnessState::Stale);
}

#[test]
fn derive_freshness_invalid_on_empty_provenance_fields() {
    let node = NodeId::Symbol(SymbolNodeId(1));
    let mut entry = sample_entry(node, "hash");
    entry.provenance.model_identity = String::new();

    assert_eq!(derive_freshness(&entry, "hash"), FreshnessState::Invalid);
}
