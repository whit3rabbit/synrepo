//! Integration tests for the sqlite-backed overlay store.

use super::{derive_freshness, SqliteOverlayStore};
use crate::core::ids::{ConceptNodeId, FileNodeId, NodeId, SymbolNodeId};
use crate::overlay::{
    CitedSpan, CommentaryEntry, CommentaryProvenance, ConfidenceTier, CrossLinkProvenance,
    FreshnessState, OverlayEdgeKind, OverlayEpistemic, OverlayLink, OverlayStore,
};
use tempfile::tempdir;
use time::OffsetDateTime;

fn sample_entry(node_id: NodeId, hash: &str) -> CommentaryEntry {
    CommentaryEntry {
        node_id,
        text: "This symbol handles user login.".to_string(),
        provenance: CommentaryProvenance {
            source_content_hash: hash.to_string(),
            pass_id: "commentary-v1".to_string(),
            model_identity: "claude-sonnet-4-6".to_string(),
            generated_at: OffsetDateTime::from_unix_timestamp(1_712_000_000).unwrap(),
        },
    }
}

#[test]
fn round_trip_insert_and_retrieve() {
    let dir = tempdir().unwrap();
    let mut store = SqliteOverlayStore::open(dir.path()).unwrap();

    let node = NodeId::Symbol(SymbolNodeId(0xabc));
    let entry = sample_entry(node, "hash-v1");
    store.insert_commentary(entry.clone()).unwrap();

    let retrieved = store.commentary_for(node).unwrap().unwrap();
    assert_eq!(retrieved.node_id, node);
    assert_eq!(retrieved.text, entry.text);
    assert_eq!(retrieved.provenance.source_content_hash, "hash-v1");
    assert_eq!(retrieved.provenance.pass_id, "commentary-v1");
    assert_eq!(retrieved.provenance.model_identity, "claude-sonnet-4-6");
    assert_eq!(
        retrieved.provenance.generated_at,
        entry.provenance.generated_at
    );
}

#[test]
fn insert_upserts_on_conflict_by_node_id() {
    let dir = tempdir().unwrap();
    let mut store = SqliteOverlayStore::open(dir.path()).unwrap();

    let node = NodeId::Symbol(SymbolNodeId(42));
    store
        .insert_commentary(sample_entry(node, "hash-v1"))
        .unwrap();

    let mut updated = sample_entry(node, "hash-v2");
    updated.text = "Refreshed commentary".to_string();
    store.insert_commentary(updated.clone()).unwrap();

    let retrieved = store.commentary_for(node).unwrap().unwrap();
    assert_eq!(retrieved.text, "Refreshed commentary");
    assert_eq!(retrieved.provenance.source_content_hash, "hash-v2");
    assert_eq!(store.commentary_count().unwrap(), 1);
}

#[test]
fn commentary_for_missing_node_returns_none() {
    let dir = tempdir().unwrap();
    let store = SqliteOverlayStore::open(dir.path()).unwrap();
    let absent = NodeId::Symbol(SymbolNodeId(1));
    assert!(store.commentary_for(absent).unwrap().is_none());
}

#[test]
fn insert_rejects_missing_provenance_fields() {
    let dir = tempdir().unwrap();
    let mut store = SqliteOverlayStore::open(dir.path()).unwrap();

    let node = NodeId::Symbol(SymbolNodeId(1));
    let mut entry = sample_entry(node, "hash");
    entry.provenance.pass_id = String::new();

    let err = store.insert_commentary(entry).unwrap_err();
    assert!(
        err.to_string().contains("provenance"),
        "expected provenance error, got: {err}"
    );
}

#[test]
fn insert_rejects_empty_text() {
    let dir = tempdir().unwrap();
    let mut store = SqliteOverlayStore::open(dir.path()).unwrap();

    let node = NodeId::Symbol(SymbolNodeId(1));
    let mut entry = sample_entry(node, "hash");
    entry.text = String::new();

    assert!(store.insert_commentary(entry).is_err());
}

#[test]
fn prune_orphans_removes_entries_for_nonlive_nodes() {
    let dir = tempdir().unwrap();
    let mut store = SqliteOverlayStore::open(dir.path()).unwrap();

    let keep = NodeId::Symbol(SymbolNodeId(1));
    let gone = NodeId::Symbol(SymbolNodeId(2));
    let file_keep = NodeId::File(FileNodeId(10));

    store.insert_commentary(sample_entry(keep, "h1")).unwrap();
    store.insert_commentary(sample_entry(gone, "h2")).unwrap();
    store
        .insert_commentary(sample_entry(file_keep, "h3"))
        .unwrap();

    let live = vec![keep, file_keep];
    let removed = store.prune_orphans(&live).unwrap();
    assert_eq!(removed, 1);

    assert!(store.commentary_for(keep).unwrap().is_some());
    assert!(store.commentary_for(gone).unwrap().is_none());
    assert!(store.commentary_for(file_keep).unwrap().is_some());
}

#[test]
fn schema_does_not_create_graph_tables() {
    let dir = tempdir().unwrap();
    let store = SqliteOverlayStore::open(dir.path()).unwrap();

    let conn = store.conn.lock();
    for table in ["files", "symbols", "concepts", "edges"] {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            count, 0,
            "overlay db must not contain graph table `{table}`"
        );
    }
    // And the overlay table does exist.
    let commentary_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='commentary'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(commentary_count, 1);
}

#[test]
fn open_existing_requires_prior_materialization() {
    let dir = tempdir().unwrap();
    // open_existing fails before any open/materialization.
    assert!(SqliteOverlayStore::open_existing(dir.path()).is_err());

    // After open() the db file exists; open_existing succeeds.
    drop(SqliteOverlayStore::open(dir.path()).unwrap());
    assert!(SqliteOverlayStore::open_existing(dir.path()).is_ok());
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

// ---------- cross-link tests ----------

fn sample_link(from: NodeId, to: NodeId, from_hash: &str, to_hash: &str) -> OverlayLink {
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
            generated_at: OffsetDateTime::from_unix_timestamp(1_712_000_000).unwrap(),
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
    store.insert_commentary(sample_entry(sym, "h1")).unwrap();

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

#[test]
fn schema_version_recorded_on_open() {
    let dir = tempdir().unwrap();
    let store = SqliteOverlayStore::open(dir.path()).unwrap();
    let conn = store.conn.lock();
    let version: String = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(version, super::CURRENT_SCHEMA_VERSION.to_string());
}

/// Overlay snapshot mirrors the graph-store contract: a reader pins a
/// committed epoch; a writer committing through a separate handle does
/// not become visible until the snapshot ends.
#[test]
fn overlay_read_snapshot_observes_one_commit_boundary() {
    let dir = tempdir().unwrap();
    let mut writer = SqliteOverlayStore::open(dir.path()).unwrap();

    let node = NodeId::Symbol(SymbolNodeId(0xbee));
    writer
        .insert_commentary(sample_entry(node, "hash-v1"))
        .unwrap();

    let reader = SqliteOverlayStore::open(dir.path()).unwrap();
    reader.begin_read_snapshot().unwrap();

    let first = reader.commentary_for(node).unwrap().unwrap();
    assert_eq!(first.provenance.source_content_hash, "hash-v1");

    // Writer refreshes via the upsert path while the reader's snapshot is
    // still open.
    writer
        .insert_commentary(sample_entry(node, "hash-v2"))
        .unwrap();

    let during = reader.commentary_for(node).unwrap().unwrap();
    assert_eq!(
        during.provenance.source_content_hash, "hash-v1",
        "reader snapshot must isolate from concurrent overlay refresh"
    );

    reader.end_read_snapshot().unwrap();

    let after = reader.commentary_for(node).unwrap().unwrap();
    assert_eq!(after.provenance.source_content_hash, "hash-v2");
}

/// Nested overlay snapshots must share the outer epoch and only release
/// when the matching depth drains to zero.
#[test]
fn overlay_nested_snapshots_share_the_outer_epoch() {
    let dir = tempdir().unwrap();
    let mut writer = SqliteOverlayStore::open(dir.path()).unwrap();

    let node = NodeId::Symbol(SymbolNodeId(1));
    writer
        .insert_commentary(sample_entry(node, "hash-v1"))
        .unwrap();

    let reader = SqliteOverlayStore::open(dir.path()).unwrap();
    reader.begin_read_snapshot().unwrap();
    reader.begin_read_snapshot().unwrap();

    // Pin the WAL snapshot with an early read; `BEGIN DEFERRED` upgrades
    // to a read transaction on the first SELECT, not on begin.
    assert_eq!(
        reader
            .commentary_for(node)
            .unwrap()
            .unwrap()
            .provenance
            .source_content_hash,
        "hash-v1"
    );

    writer
        .insert_commentary(sample_entry(node, "hash-v2"))
        .unwrap();

    reader.end_read_snapshot().unwrap();
    assert_eq!(
        reader
            .commentary_for(node)
            .unwrap()
            .unwrap()
            .provenance
            .source_content_hash,
        "hash-v1",
        "inner end must not release the outer snapshot"
    );

    reader.end_read_snapshot().unwrap();
    assert_eq!(
        reader
            .commentary_for(node)
            .unwrap()
            .unwrap()
            .provenance
            .source_content_hash,
        "hash-v2"
    );
}

/// Overlay `end_read_snapshot` without a matching begin must be a no-op.
#[test]
fn overlay_end_without_begin_is_noop() {
    let dir = tempdir().unwrap();
    let store = SqliteOverlayStore::open(dir.path()).unwrap();
    store.end_read_snapshot().unwrap();
    store.end_read_snapshot().unwrap();
}

// ---------- compact tests ----------

fn old_entry(node_id: NodeId, days_ago: i64) -> CommentaryEntry {
    let old_timestamp = OffsetDateTime::now_utc() - time::Duration::days(days_ago);
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
            generated_at: OffsetDateTime::now_utc(),
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

    let policy = crate::pipeline::maintenance::CompactPolicy::Default;
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

    let policy = crate::pipeline::maintenance::CompactPolicy::Default;
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

    let policy = crate::pipeline::maintenance::CompactPolicy::Aggressive;
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
