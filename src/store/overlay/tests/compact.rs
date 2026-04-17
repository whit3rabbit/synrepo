//! Compact policy and candidate selection tests.

use crate::core::ids::{ConceptNodeId, NodeId, SymbolNodeId};
use crate::overlay::{
    CommentaryEntry, CommentaryProvenance, ConfidenceTier, OverlayLink, OverlayStore,
};
use crate::pipeline::maintenance::CompactPolicy;
use crate::store::overlay::SqliteOverlayStore;
use tempfile::tempdir;

fn sample_link(from: NodeId, to: NodeId, from_hash: &str, to_hash: &str) -> OverlayLink {
    crate::overlay::OverlayLink {
        from,
        to,
        kind: crate::overlay::OverlayEdgeKind::References,
        epistemic: crate::overlay::OverlayEpistemic::MachineAuthoredHighConf,
        source_spans: vec![crate::overlay::CitedSpan {
            artifact: from,
            normalized_text: "authenticate".into(),
            verified_at_offset: 12,
            lcs_ratio: 0.98,
        }],
        target_spans: vec![crate::overlay::CitedSpan {
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
        provenance: crate::overlay::CrossLinkProvenance {
            pass_id: "cross-link-v1".into(),
            model_identity: "claude-sonnet-4-6".into(),
            generated_at: time::OffsetDateTime::from_unix_timestamp(1_712_000_000).unwrap(),
        },
    }
}

fn old_entry(node_id: NodeId, days_ago: i64) -> CommentaryEntry {
    let old_timestamp = time::OffsetDateTime::now_utc() - time::Duration::days(days_ago);
    CommentaryEntry {
        node_id,
        text: format!("Old commentary from {} days ago", days_ago),
        provenance: CommentaryProvenance {
            source_content_hash: "hash-old".to_string(),
            pass_id: "commentary-v1".to_string(),
            model_identity: "claude-sonnet-4-6".to_string(),
            generated_at: old_timestamp,
        },
    }
}

fn fresh_entry(node_id: NodeId) -> CommentaryEntry {
    CommentaryEntry {
        node_id,
        text: "Fresh commentary".to_string(),
        provenance: CommentaryProvenance {
            source_content_hash: "hash-fresh".to_string(),
            pass_id: "commentary-v1".to_string(),
            model_identity: "claude-sonnet-4-6".to_string(),
            generated_at: time::OffsetDateTime::now_utc(),
        },
    }
}

#[test]
fn compactable_commentary_stats_reflects_stale_vs_active() {
    let dir = tempdir().unwrap();
    let mut store = SqliteOverlayStore::open(dir.path()).unwrap();

    let stale_node = NodeId::Symbol(SymbolNodeId(1));
    let fresh_node = NodeId::Symbol(SymbolNodeId(2));

    // Insert stale entry (60 days old).
    store.insert_commentary(old_entry(stale_node, 60)).unwrap();
    // Insert fresh entry (today).
    store.insert_commentary(fresh_entry(fresh_node)).unwrap();

    let policy = CompactPolicy::Default;
    let stats = store.compactable_commentary_stats(&policy).unwrap();

    // Only the 60-day-old entry should be compactable under Default policy (30-day window).
    assert_eq!(stats.compactable_commentary, 1);
}

#[test]
fn compact_commentary_drops_only_stale_entries() {
    let dir = tempdir().unwrap();
    let mut store = SqliteOverlayStore::open(dir.path()).unwrap();

    let stale_node = NodeId::Symbol(SymbolNodeId(1));
    let fresh_node = NodeId::Symbol(SymbolNodeId(2));

    store.insert_commentary(old_entry(stale_node, 60)).unwrap();
    store.insert_commentary(fresh_entry(fresh_node)).unwrap();

    let policy = CompactPolicy::Default;
    let deleted = store.compact_commentary(&policy).unwrap();

    assert_eq!(deleted, 1);

    // Verify stale entry is gone, fresh entry remains.
    assert!(store.commentary_for(stale_node).unwrap().is_none());
    assert!(store.commentary_for(fresh_node).unwrap().is_some());
}

#[test]
fn compact_commentary_never_drops_active_entries() {
    let dir = tempdir().unwrap();
    let mut store = SqliteOverlayStore::open(dir.path()).unwrap();

    let fresh_node = NodeId::Symbol(SymbolNodeId(1));
    store.insert_commentary(fresh_entry(fresh_node)).unwrap();

    let policy = CompactPolicy::Aggressive;
    let deleted = store.compact_commentary(&policy).unwrap();

    assert_eq!(deleted, 0);
    assert!(store.commentary_for(fresh_node).unwrap().is_some());
}

/// Verify that `candidates_limited` applies the limit at the SQL layer
/// and returns fewer rows than `all_candidates` when the corpus exceeds the limit.
#[test]
fn candidates_limited_applies_sql_side_limit() {
    let dir = tempdir().unwrap();
    let mut store = SqliteOverlayStore::open(dir.path()).unwrap();

    // Insert 10 cross-link candidates with varying scores.
    for i in 0..10u64 {
        let from = NodeId::Concept(ConceptNodeId(i));
        let to = NodeId::Symbol(SymbolNodeId(100 + i));
        let mut link = sample_link(from, to, "h-from", "h-to");
        link.confidence_score = 0.5 + (i as f32 * 0.04); // 0.50 .. 0.86
        link.confidence_tier = if i < 5 {
            ConfidenceTier::ReviewQueue
        } else {
            ConfidenceTier::High
        };
        store.insert_link(link).unwrap();
    }

    // all_candidates returns all 10.
    let all = store.all_candidates(None).unwrap();
    assert_eq!(all.len(), 10);

    // candidates_limited with limit=3 returns exactly 3, ordered by score DESC.
    let limited = store.candidates_limited(None, 3).unwrap();
    assert_eq!(limited.len(), 3);
    // Highest scores should be first (0.86, 0.82, 0.78).
    assert!(limited[0].confidence_score > limited[1].confidence_score);
    assert!(limited[1].confidence_score > limited[2].confidence_score);

    // Filter by tier: review_queue has 5 entries.
    let review_all = store.all_candidates(Some("review_queue")).unwrap();
    assert_eq!(review_all.len(), 5);

    let review_limited = store.candidates_limited(Some("review_queue"), 2).unwrap();
    assert_eq!(review_limited.len(), 2);
}
