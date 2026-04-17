//! Node CRUD operations for SqliteGraphStore.

use crate::{
    core::ids::{ConceptNodeId, FileNodeId, SymbolNodeId},
    structure::graph::{ConceptNode, FileNode, SymbolNode},
};

use super::super::{
    codec::{encode_json, encode_label, load_row},
    SqliteGraphStore,
};

use rusqlite::params;

/// Upsert a file node.
pub fn upsert_file(store: &mut SqliteGraphStore, node: FileNode) -> crate::Result<()> {
    let data = encode_json(&node)?;
    let id = node.id.0 as i64;
    let path = node.path;
    let last_obs = node.last_observed_rev.map(|r| r as i64);

    store.conn.lock().execute(
        "INSERT INTO files (id, path, last_observed_rev, data)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(id) DO UPDATE SET
             path = excluded.path,
             last_observed_rev = excluded.last_observed_rev,
             data = excluded.data",
        params![id, path, last_obs, data],
    )?;
    Ok(())
}

/// Upsert a symbol node.
pub fn upsert_symbol(store: &mut SqliteGraphStore, node: SymbolNode) -> crate::Result<()> {
    let data = encode_json(&node)?;
    let kind = encode_label(&node.kind)?;
    let id = node.id.0 as i64;
    let file_id = node.file_id.0 as i64;
    let qualified_name = node.qualified_name;
    let first_seen_rev = &node.first_seen_rev;
    let last_modified_rev = &node.last_modified_rev;
    let last_obs = node.last_observed_rev.map(|r| r as i64);
    let retired = node.retired_at_rev.map(|r| r as i64);

    store.conn.lock().execute(
        "INSERT INTO symbols (id, file_id, qualified_name, kind, first_seen_rev, last_modified_rev, last_observed_rev, retired_at_rev, data)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(id) DO UPDATE SET
             file_id = excluded.file_id,
             qualified_name = excluded.qualified_name,
             kind = excluded.kind,
             first_seen_rev = excluded.first_seen_rev,
             last_modified_rev = excluded.last_modified_rev,
             last_observed_rev = excluded.last_observed_rev,
             retired_at_rev = excluded.retired_at_rev,
             data = excluded.data",
        params![id, file_id, qualified_name, kind, first_seen_rev, last_modified_rev, last_obs, retired, data],
    )?;
    Ok(())
}

/// Upsert a concept node.
pub fn upsert_concept(store: &mut SqliteGraphStore, node: ConceptNode) -> crate::Result<()> {
    let data = encode_json(&node)?;
    let id = node.id.0 as i64;
    let path = node.path;
    let last_obs = node.last_observed_rev.map(|r| r as i64);

    store.conn.lock().execute(
        "INSERT INTO concepts (id, path, last_observed_rev, data)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(id) DO UPDATE SET
             path = excluded.path,
             last_observed_rev = excluded.last_observed_rev,
             data = excluded.data",
        params![id, path, last_obs, data],
    )?;
    Ok(())
}

/// Delete a node (file, symbol, or concept) and all associated edges.
pub fn delete_node(
    store: &mut SqliteGraphStore,
    id: crate::core::ids::NodeId,
) -> crate::Result<()> {
    let conn = store.conn.lock();
    super::helpers::delete_node_inner(&conn, id)
}

/// Get a file node by ID.
pub fn get_file(store: &SqliteGraphStore, id: FileNodeId) -> crate::Result<Option<FileNode>> {
    let conn = store.conn.lock();
    load_row(
        &conn,
        "SELECT data FROM files WHERE id = ?1",
        params![id.0 as i64],
    )
}

/// Get a symbol node by ID.
pub fn get_symbol(store: &SqliteGraphStore, id: SymbolNodeId) -> crate::Result<Option<SymbolNode>> {
    let conn = store.conn.lock();
    load_row(
        &conn,
        "SELECT data FROM symbols WHERE id = ?1",
        params![id.0 as i64],
    )
}

/// Get a concept node by ID.
pub fn get_concept(
    store: &SqliteGraphStore,
    id: ConceptNodeId,
) -> crate::Result<Option<ConceptNode>> {
    let conn = store.conn.lock();
    load_row(
        &conn,
        "SELECT data FROM concepts WHERE id = ?1",
        params![id.0 as i64],
    )
}

/// Get a file node by path.
pub fn file_by_path(store: &SqliteGraphStore, path: &str) -> crate::Result<Option<FileNode>> {
    let conn = store.conn.lock();
    load_row(
        &conn,
        "SELECT data FROM files WHERE path = ?1",
        params![path],
    )
}
