//! Edge CRUD and traversal operations for SqliteGraphStore.

use crate::{
    core::ids::{EdgeId, NodeId},
    structure::graph::{Edge, EdgeKind},
};

use super::super::{
    codec::{encode_json, encode_label, load_rows},
    SqliteGraphStore,
};

use rusqlite::params;

/// Insert or update an edge.
pub fn insert_edge(store: &mut SqliteGraphStore, edge: Edge) -> crate::Result<()> {
    let data = encode_json(&edge)?;
    let kind = encode_label(&edge.kind)?;
    let id = edge.id.0 as i64;
    let from_node_id = edge.from.to_string();
    let to_node_id = edge.to.to_string();
    let owner_fid = edge.owner_file_id.map(|fid| fid.0 as i64);
    let last_obs = edge.last_observed_rev.map(|r| r as i64);
    let retired = edge.retired_at_rev.map(|r| r as i64);

    store.conn.lock().execute(
        "INSERT INTO edges (id, from_node_id, to_node_id, kind, owner_file_id, last_observed_rev, retired_at_rev, data)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(id) DO UPDATE SET
             from_node_id = excluded.from_node_id,
             to_node_id = excluded.to_node_id,
             kind = excluded.kind,
             owner_file_id = excluded.owner_file_id,
             last_observed_rev = excluded.last_observed_rev,
             retired_at_rev = excluded.retired_at_rev,
             data = excluded.data",
        params![id, from_node_id, to_node_id, kind, owner_fid, last_obs, retired, data],
    )?;
    Ok(())
}

/// Delete an edge by ID.
pub fn delete_edge(store: &mut SqliteGraphStore, edge_id: EdgeId) -> crate::Result<()> {
    let id = edge_id.0 as i64;
    store
        .conn
        .lock()
        .execute("DELETE FROM edges WHERE id = ?1", params![id])?;
    Ok(())
}

/// Delete all edges of a given kind. Returns count of deleted edges.
pub fn delete_edges_by_kind(store: &mut SqliteGraphStore, kind: EdgeKind) -> crate::Result<usize> {
    let kind_label = encode_label(&kind)?;
    let count = store
        .conn
        .lock()
        .execute("DELETE FROM edges WHERE kind = ?1", params![kind_label])?;
    Ok(count)
}

/// Get all edges outbound from a node, optionally filtered by kind.
pub fn outbound(
    store: &SqliteGraphStore,
    from: NodeId,
    kind: Option<EdgeKind>,
) -> crate::Result<Vec<Edge>> {
    let conn = store.conn.lock();
    let from_node_id = from.to_string();

    if let Some(kind) = kind {
        let kind = encode_label(&kind)?;
        load_rows(
            &conn,
            "SELECT data FROM edges WHERE from_node_id = ?1 AND kind = ?2 AND retired_at_rev IS NULL ORDER BY id",
            params![from_node_id, kind],
        )
    } else {
        load_rows(
            &conn,
            "SELECT data FROM edges WHERE from_node_id = ?1 AND retired_at_rev IS NULL ORDER BY id",
            params![from_node_id],
        )
    }
}

/// Get all edges inbound to a node, optionally filtered by kind.
pub fn inbound(
    store: &SqliteGraphStore,
    to: NodeId,
    kind: Option<EdgeKind>,
) -> crate::Result<Vec<Edge>> {
    let conn = store.conn.lock();
    let to_node_id = to.to_string();

    if let Some(kind) = kind {
        let kind = encode_label(&kind)?;
        load_rows(
            &conn,
            "SELECT data FROM edges WHERE to_node_id = ?1 AND kind = ?2 AND retired_at_rev IS NULL ORDER BY id",
            params![to_node_id, kind],
        )
    } else {
        load_rows(
            &conn,
            "SELECT data FROM edges WHERE to_node_id = ?1 AND retired_at_rev IS NULL ORDER BY id",
            params![to_node_id],
        )
    }
}

/// Get all active edges (non-retired).
pub fn all_edges(store: &SqliteGraphStore) -> crate::Result<Vec<Edge>> {
    let conn = store.conn.lock();
    load_rows(
        &conn,
        "SELECT data FROM edges WHERE retired_at_rev IS NULL ORDER BY id",
        params![],
    )
}
