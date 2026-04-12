//! Integration tests for the sqlite-backed overlay store.

use super::{derive_freshness, SqliteOverlayStore};
use crate::core::ids::{FileNodeId, NodeId, SymbolNodeId};
use crate::overlay::{CommentaryEntry, CommentaryProvenance, FreshnessState, OverlayStore};
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
