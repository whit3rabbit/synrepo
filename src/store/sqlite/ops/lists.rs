//! List query operations for SqliteGraphStore.

use crate::core::ids::{ConceptNodeId, FileNodeId, SymbolNodeId};

use super::super::SqliteGraphStore;

/// Return type for all_symbols_summary.
type SymbolSummary = (SymbolNodeId, FileNodeId, String, String, String);

/// Get all file paths and their IDs, ordered by path.
pub fn all_file_paths(store: &SqliteGraphStore) -> crate::Result<Vec<(String, FileNodeId)>> {
    let conn = store.conn.lock();
    let mut stmt = conn.prepare_cached("SELECT path, id FROM files ORDER BY path")?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows
        .into_iter()
        .map(|(p, id)| (p, FileNodeId(id as u64)))
        .collect())
}

/// Get all concept paths and their IDs, ordered by path.
pub fn all_concept_paths(store: &SqliteGraphStore) -> crate::Result<Vec<(String, ConceptNodeId)>> {
    let conn = store.conn.lock();
    let mut stmt = conn.prepare_cached("SELECT path, id FROM concepts ORDER BY path")?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows
        .into_iter()
        .map(|(p, id)| (p, ConceptNodeId(id as u64)))
        .collect())
}

/// Get all active symbol IDs, their file IDs, and qualified names.
pub fn all_symbol_names(
    store: &SqliteGraphStore,
) -> crate::Result<Vec<(SymbolNodeId, FileNodeId, String)>> {
    let conn = store.conn.lock();
    let mut stmt = conn.prepare_cached(
        "SELECT id, file_id, qualified_name FROM symbols WHERE retired_at_rev IS NULL ORDER BY id",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows
        .into_iter()
        .map(|(sym_id, file_id, name)| {
            (
                SymbolNodeId(sym_id as u64),
                FileNodeId(file_id as u64),
                name,
            )
        })
        .collect())
}

/// Get all active symbol summaries (ID, file ID, name, kind, body_hash).
/// Uses json_extract to avoid full deserialization.
pub fn all_symbols_summary(store: &SqliteGraphStore) -> crate::Result<Vec<SymbolSummary>> {
    let conn = store.conn.lock();
    // body_hash is stored inside the JSON `data` blob, not a dedicated column.
    // json_extract avoids a full deserialization while staying in one query.
    let mut stmt = conn.prepare_cached(
        "SELECT id, file_id, qualified_name, kind, \
         json_extract(data, '$.body_hash') FROM symbols WHERE retired_at_rev IS NULL ORDER BY id",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows
        .into_iter()
        .map(|(sym_id, file_id, qname, kind, body_hash)| {
            (
                SymbolNodeId(sym_id as u64),
                FileNodeId(file_id as u64),
                qname,
                kind,
                body_hash,
            )
        })
        .collect())
}
