//! Reader-consistency tests for `SqliteGraphStore` read snapshots.
//!
//! These tests pin down the core property the snapshot API exists for: a
//! multi-query read through a single handle observes exactly one committed
//! epoch, even when a different handle commits in between queries.

use super::super::SqliteGraphStore;
use super::support::sample_provenance;
use crate::{
    core::ids::{FileNodeId, NodeId, SymbolNodeId},
    structure::graph::{Epistemic, FileNode, GraphStore, SymbolKind, SymbolNode},
};
use tempfile::tempdir;

fn make_file(id: u64, path: &str) -> FileNode {
    FileNode {
        id: FileNodeId(id),
        path: path.to_string(),
        path_history: Vec::new(),
        content_hash: format!("hash-{id:x}"),
        size_bytes: 1,
        language: Some("rust".to_string()),
        inline_decisions: Vec::new(),
        last_observed_rev: None,
        epistemic: Epistemic::ParserObserved,
        provenance: sample_provenance("parse_code", path),
    }
}

fn make_symbol(id: u64, file_id: FileNodeId, qname: &str) -> SymbolNode {
    SymbolNode {
        id: SymbolNodeId(id),
        file_id,
        qualified_name: qname.to_string(),
        display_name: qname.to_string(),
        kind: SymbolKind::Module,
        body_byte_range: (0, 1),
        body_hash: format!("body-{id:x}"),
        signature: None,
        doc_comment: None,
        first_seen_rev: None,
        last_modified_rev: None,
        last_observed_rev: None,
        retired_at_rev: None,
        epistemic: Epistemic::ParserObserved,
        provenance: sample_provenance("parse_code", "src/lib.rs"),
    }
}

/// Reader pins an epoch at `begin_read_snapshot`; a concurrent writer commit
/// on a *different* handle to the same DB is invisible until the snapshot
/// ends. After `end_read_snapshot`, new queries see the post-writer state.
#[test]
fn read_snapshot_observes_one_commit_boundary() {
    let dir = tempdir().unwrap();
    let graph_dir = dir.path().join(".synrepo/graph");

    // Writer seeds the DB with F1 and S1.
    let mut writer = SqliteGraphStore::open(&graph_dir).unwrap();
    let file = make_file(0x42, "src/lib.rs");
    let sym = make_symbol(0x24, file.id, "crate::lib");
    writer.begin().unwrap();
    writer.upsert_file(file.clone()).unwrap();
    writer.upsert_symbol(sym.clone()).unwrap();
    writer.commit().unwrap();

    // Reader opens a separate handle to the same DB.
    let reader = SqliteGraphStore::open(&graph_dir).unwrap();

    reader.begin_read_snapshot().unwrap();

    // First read inside the snapshot sees the seeded node.
    assert!(reader.get_file(file.id).unwrap().is_some());

    // Writer deletes the file and commits while the reader's snapshot is
    // still open.
    writer.delete_node(NodeId::File(file.id)).unwrap();

    // The reader still sees pre-delete state for the entire snapshot,
    // including follow-up queries that would otherwise land on a later
    // committed epoch.
    let still_present = reader.get_file(file.id).unwrap();
    assert!(
        still_present.is_some(),
        "reader snapshot should isolate from concurrent delete"
    );

    reader.end_read_snapshot().unwrap();

    // Post-snapshot, the reader now sees the deletion.
    assert!(reader.get_file(file.id).unwrap().is_none());
}

/// Nested `begin_read_snapshot` calls on the same handle are safe: both
/// levels share the outer committed epoch, and the snapshot stays open
/// until the matching `end_read_snapshot` count drains to zero. This is
/// the property that lets an MCP handler wrap its body while
/// `GraphCardCompiler` also wraps internally without tripping
/// SQLite's "transaction within a transaction" error.
#[test]
fn nested_snapshots_share_the_outer_epoch() {
    let dir = tempdir().unwrap();
    let graph_dir = dir.path().join(".synrepo/graph");

    let mut writer = SqliteGraphStore::open(&graph_dir).unwrap();
    let file = make_file(0x10, "src/a.rs");
    writer.begin().unwrap();
    writer.upsert_file(file.clone()).unwrap();
    writer.commit().unwrap();

    let reader = SqliteGraphStore::open(&graph_dir).unwrap();

    // Outer begin.
    reader.begin_read_snapshot().unwrap();
    // Inner begin must not error (would, on a non-re-entrant impl, with
    // "cannot start a transaction within a transaction").
    reader.begin_read_snapshot().unwrap();

    // Pin the WAL snapshot now: `BEGIN DEFERRED` only upgrades to a read
    // transaction on the first SELECT, so an early read is what binds the
    // reader to the current committed epoch.
    assert!(reader.get_file(file.id).unwrap().is_some());

    // Writer commits a new file that should be invisible to both levels.
    let file2 = make_file(0x11, "src/b.rs");
    writer.begin().unwrap();
    writer.upsert_file(file2.clone()).unwrap();
    writer.commit().unwrap();

    // Inner end pops depth but does not release the snapshot.
    reader.end_read_snapshot().unwrap();
    assert!(
        reader.get_file(file2.id).unwrap().is_none(),
        "inner end must not release the outer snapshot"
    );

    // Outer end releases the snapshot; next reads see post-commit state.
    reader.end_read_snapshot().unwrap();
    assert!(reader.get_file(file2.id).unwrap().is_some());
}

/// Calling `end_read_snapshot` with no snapshot active must be a no-op
/// so the `with_graph_read_snapshot` helper's error-path cleanup can't
/// mask the caller's original error.
#[test]
fn end_without_begin_is_noop() {
    let dir = tempdir().unwrap();
    let graph_dir = dir.path().join(".synrepo/graph");

    let reader = SqliteGraphStore::open(&graph_dir).unwrap();
    reader.end_read_snapshot().unwrap();
    reader.end_read_snapshot().unwrap();
}
