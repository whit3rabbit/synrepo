//! Snapshot isolation tests.

use crate::core::ids::{NodeId, SymbolNodeId};
use crate::overlay::{CommentaryEntry, CommentaryProvenance, OverlayStore};
use crate::store::overlay::SqliteOverlayStore;
use tempfile::tempdir;

fn sample_commentary_entry(node_id: NodeId, hash: &str) -> CommentaryEntry {
    CommentaryEntry {
        node_id,
        text: "Sample text".to_string(),
        provenance: CommentaryProvenance {
            source_content_hash: hash.to_string(),
            pass_id: "commentary-v1".to_string(),
            model_identity: "claude-sonnet-4-6".to_string(),
            generated_at: time::OffsetDateTime::from_unix_timestamp(1_712_000_000).unwrap(),
        },
    }
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
        .insert_commentary(sample_commentary_entry(node, "hash-v1"))
        .unwrap();

    let reader = SqliteOverlayStore::open(dir.path()).unwrap();
    reader.begin_read_snapshot().unwrap();

    let first = reader.commentary_for(node).unwrap().unwrap();
    assert_eq!(first.provenance.source_content_hash, "hash-v1");

    // Writer refreshes via the upsert path while the reader's snapshot is
    // still open.
    writer
        .insert_commentary(sample_commentary_entry(node, "hash-v2"))
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
        .insert_commentary(sample_commentary_entry(node, "hash-v1"))
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
        .insert_commentary(sample_commentary_entry(node, "hash-v2"))
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
