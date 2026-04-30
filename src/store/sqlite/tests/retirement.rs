//! Tests for the observation lifecycle: compile revisions, retirement,
//! unretirement, ownership queries, and compaction.

use super::support::sample_provenance;
use crate::{
    core::ids::{FileNodeId, NodeId, SymbolNodeId},
    store::sqlite::SqliteGraphStore,
    structure::graph::{
        derive_edge_id, Edge, EdgeKind, Epistemic, FileNode, GraphStore, SymbolNode, Visibility,
    },
};

fn open_memory_store() -> SqliteGraphStore {
    SqliteGraphStore::open_db(std::path::Path::new(":memory:")).unwrap()
}

fn make_file(id: u64, path: &str) -> FileNode {
    FileNode {
        id: FileNodeId(id as u128),
        root_id: "primary".to_string(),
        path: path.to_string(),
        path_history: vec![],
        content_hash: format!("hash_{id}"),
        size_bytes: 100,
        language: Some("rust".to_string()),
        inline_decisions: vec![],
        last_observed_rev: Some(1),
        epistemic: Epistemic::ParserObserved,
        provenance: sample_provenance("discover", path),
    }
}

fn make_symbol(id: u64, file_id: u64, name: &str) -> SymbolNode {
    SymbolNode {
        id: SymbolNodeId(id as u128),
        file_id: FileNodeId(file_id as u128),
        qualified_name: name.to_string(),
        display_name: name.to_string(),
        kind: crate::structure::graph::SymbolKind::Function,
        visibility: Visibility::Public,
        body_byte_range: (0, 10),
        body_hash: format!("body_{id}"),
        signature: None,
        doc_comment: None,
        first_seen_rev: None,
        last_modified_rev: None,
        last_observed_rev: Some(1),
        retired_at_rev: None,
        epistemic: Epistemic::ParserObserved,
        provenance: sample_provenance("parse_code", "test.rs"),
    }
}

fn make_defines_edge(file_id: u64, sym_id: u64) -> Edge {
    let from = NodeId::File(FileNodeId(file_id as u128));
    let to = NodeId::Symbol(SymbolNodeId(sym_id as u128));
    Edge {
        id: derive_edge_id(from, to, EdgeKind::Defines),
        from,
        to,
        kind: EdgeKind::Defines,
        owner_file_id: Some(FileNodeId(file_id as u128)),
        last_observed_rev: Some(1),
        retired_at_rev: None,
        epistemic: Epistemic::ParserObserved,
        drift_score: 0.0,
        provenance: sample_provenance("parse_code", "test.rs"),
    }
}

#[test]
fn compile_revision_increments() {
    let mut store = open_memory_store();
    let r1 = store.next_compile_revision().unwrap();
    let r2 = store.next_compile_revision().unwrap();
    assert!(r2 > r1);
}

#[test]
fn retire_symbols_bulk_matches_per_symbol_loop() {
    let mut bulk = open_memory_store();
    let mut serial = open_memory_store();
    bulk.upsert_file(make_file(1, "a.rs")).unwrap();
    serial.upsert_file(make_file(1, "a.rs")).unwrap();
    let ids: Vec<SymbolNodeId> = (10..30u64)
        .map(|i| {
            bulk.upsert_symbol(make_symbol(i, 1, &format!("s{i}")))
                .unwrap();
            serial
                .upsert_symbol(make_symbol(i, 1, &format!("s{i}")))
                .unwrap();
            SymbolNodeId(i as u128)
        })
        .collect();

    // Bulk retire vs per-row retire at the same revision.
    bulk.retire_symbols_bulk(&ids, 2).unwrap();
    for id in &ids {
        serial.retire_symbol(*id, 2).unwrap();
    }

    // Active symbols match.
    let bulk_active = bulk.symbols_for_file(FileNodeId(1_u128)).unwrap();
    let serial_active = serial.symbols_for_file(FileNodeId(1_u128)).unwrap();
    assert_eq!(bulk_active.len(), serial_active.len());
    assert!(bulk_active.is_empty(), "all should be retired");

    // Each retired row carries the same retired_at_rev.
    for id in &ids {
        let b = bulk.get_symbol(*id).unwrap().unwrap();
        let s = serial.get_symbol(*id).unwrap().unwrap();
        assert_eq!(b.retired_at_rev, s.retired_at_rev);
        assert_eq!(b.retired_at_rev, Some(2));
    }
}

#[test]
fn retire_symbols_bulk_handles_empty_input() {
    let mut store = open_memory_store();
    store.upsert_file(make_file(1, "a.rs")).unwrap();
    store.upsert_symbol(make_symbol(10, 1, "foo")).unwrap();

    store.retire_symbols_bulk(&[], 2).unwrap();
    // Existing symbol unaffected.
    assert_eq!(store.symbols_for_file(FileNodeId(1_u128)).unwrap().len(), 1);
}

#[test]
fn retire_symbol_hides_from_symbols_for_file() {
    let mut store = open_memory_store();
    store.upsert_file(make_file(1, "a.rs")).unwrap();
    store.upsert_symbol(make_symbol(10, 1, "foo")).unwrap();
    store.upsert_symbol(make_symbol(11, 1, "bar")).unwrap();

    // Both visible before retirement.
    assert_eq!(store.symbols_for_file(FileNodeId(1_u128)).unwrap().len(), 2);

    // Retire one.
    store.retire_symbol(SymbolNodeId(10_u128), 2).unwrap();

    // Only one visible.
    let active = store.symbols_for_file(FileNodeId(1_u128)).unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].qualified_name, "bar");

    // But get_symbol still finds the retired one (for drift, provenance).
    let retired = store.get_symbol(SymbolNodeId(10_u128)).unwrap().unwrap();
    assert_eq!(retired.retired_at_rev, Some(2));
}

#[test]
fn retire_edge_hides_from_outbound_and_active_edges() {
    let mut store = open_memory_store();
    store.upsert_file(make_file(1, "a.rs")).unwrap();
    store.upsert_symbol(make_symbol(10, 1, "foo")).unwrap();

    let edge = make_defines_edge(1, 10);
    store.insert_edge(edge.clone()).unwrap();

    // Visible before retirement.
    assert_eq!(
        store
            .outbound(NodeId::File(FileNodeId(1_u128)), None)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(store.active_edges().unwrap().len(), 1);

    // Retire.
    store.retire_edge(edge.id, 2).unwrap();

    // Hidden from outbound and active_edges.
    assert_eq!(
        store
            .outbound(NodeId::File(FileNodeId(1_u128)), None)
            .unwrap()
            .len(),
        0
    );
    assert_eq!(store.active_edges().unwrap().len(), 0);

    // Retired edges are excluded from all_edges (drift scoring operates
    // only on active edges).
    assert_eq!(store.all_edges().unwrap().len(), 0);
}

#[test]
fn edges_owned_by_returns_only_active_edges_for_owner() {
    let mut store = open_memory_store();
    store.upsert_file(make_file(1, "a.rs")).unwrap();
    store.upsert_file(make_file(2, "b.rs")).unwrap();
    store.upsert_symbol(make_symbol(10, 1, "foo")).unwrap();
    store.upsert_symbol(make_symbol(20, 2, "bar")).unwrap();

    store.insert_edge(make_defines_edge(1, 10)).unwrap();
    store.insert_edge(make_defines_edge(2, 20)).unwrap();

    assert_eq!(store.edges_owned_by(FileNodeId(1_u128)).unwrap().len(), 1);
    assert_eq!(store.edges_owned_by(FileNodeId(2_u128)).unwrap().len(), 1);

    // Retire edge owned by file 1.
    let e1 = make_defines_edge(1, 10);
    store.retire_edge(e1.id, 2).unwrap();

    assert_eq!(store.edges_owned_by(FileNodeId(1_u128)).unwrap().len(), 0);
    assert_eq!(store.edges_owned_by(FileNodeId(2_u128)).unwrap().len(), 1);
}

#[test]
fn unretire_symbol_restores_visibility() {
    let mut store = open_memory_store();
    store.upsert_file(make_file(1, "a.rs")).unwrap();
    store.upsert_symbol(make_symbol(10, 1, "foo")).unwrap();

    store.retire_symbol(SymbolNodeId(10_u128), 2).unwrap();
    assert_eq!(store.symbols_for_file(FileNodeId(1_u128)).unwrap().len(), 0);

    store.unretire_symbol(SymbolNodeId(10_u128), 3).unwrap();
    let active = store.symbols_for_file(FileNodeId(1_u128)).unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].last_observed_rev, Some(3));
    assert_eq!(active[0].retired_at_rev, None);
}

#[test]
fn unretire_edge_restores_visibility() {
    let mut store = open_memory_store();
    store.upsert_file(make_file(1, "a.rs")).unwrap();
    store.upsert_symbol(make_symbol(10, 1, "foo")).unwrap();

    let edge = make_defines_edge(1, 10);
    store.insert_edge(edge.clone()).unwrap();

    store.retire_edge(edge.id, 2).unwrap();
    assert_eq!(store.active_edges().unwrap().len(), 0);

    store.unretire_edge(edge.id, 3).unwrap();
    let active = store.active_edges().unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].last_observed_rev, Some(3));
    assert_eq!(active[0].retired_at_rev, None);
}

#[test]
fn compact_retired_removes_old_observations() {
    let mut store = open_memory_store();
    store.upsert_file(make_file(1, "a.rs")).unwrap();
    store.upsert_symbol(make_symbol(10, 1, "foo")).unwrap();
    store.upsert_symbol(make_symbol(11, 1, "bar")).unwrap();

    let edge = make_defines_edge(1, 10);
    store.insert_edge(edge.clone()).unwrap();

    // Retire at rev 2.
    store.retire_symbol(SymbolNodeId(10_u128), 2).unwrap();
    store.retire_edge(edge.id, 2).unwrap();

    // Compact with threshold 3 (removes retired_at < 3).
    let summary = store.compact_retired(3).unwrap();
    assert_eq!(summary.symbols_removed, 1);
    assert_eq!(summary.edges_removed, 1);

    // Retired symbol is gone.
    assert!(store.get_symbol(SymbolNodeId(10_u128)).unwrap().is_none());

    // Active symbol survives.
    assert!(store.get_symbol(SymbolNodeId(11_u128)).unwrap().is_some());
}

#[test]
fn compact_retired_preserves_active_observations() {
    let mut store = open_memory_store();
    store.upsert_file(make_file(1, "a.rs")).unwrap();
    store.upsert_symbol(make_symbol(10, 1, "foo")).unwrap();

    let edge = make_defines_edge(1, 10);
    store.insert_edge(edge).unwrap();

    // Compact with any threshold: nothing is retired, nothing removed.
    let summary = store.compact_retired(100).unwrap();
    assert_eq!(summary.symbols_removed, 0);
    assert_eq!(summary.edges_removed, 0);

    assert!(store.get_symbol(SymbolNodeId(10_u128)).unwrap().is_some());
    assert_eq!(store.all_edges().unwrap().len(), 1);
}

#[test]
fn compact_retired_within_retention_window_survives() {
    let mut store = open_memory_store();
    store.upsert_file(make_file(1, "a.rs")).unwrap();
    store.upsert_symbol(make_symbol(10, 1, "foo")).unwrap();

    // Retire at rev 5.
    store.retire_symbol(SymbolNodeId(10_u128), 5).unwrap();

    // Compact with threshold 4: retired_at_rev (5) >= threshold (4), so survives.
    let summary = store.compact_retired(4).unwrap();
    assert_eq!(summary.symbols_removed, 0);

    // Compact with threshold 6: retired_at_rev (5) < threshold (6), so removed.
    let summary = store.compact_retired(6).unwrap();
    assert_eq!(summary.symbols_removed, 1);
}
