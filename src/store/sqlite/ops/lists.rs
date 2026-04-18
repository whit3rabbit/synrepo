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
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows
        .into_iter()
        .map(|(p, id)| (p, id.parse::<FileNodeId>().unwrap()))
        .collect())
}

/// Get all concept paths and their IDs, ordered by path.
pub fn all_concept_paths(store: &SqliteGraphStore) -> crate::Result<Vec<(String, ConceptNodeId)>> {
    let conn = store.conn.lock();
    let mut stmt = conn.prepare_cached("SELECT path, id FROM concepts ORDER BY path")?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows
        .into_iter()
        .map(|(p, id)| (p, id.parse::<ConceptNodeId>().unwrap()))
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
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows
        .into_iter()
        .map(|(sym_id, file_id, name)| {
            (
                sym_id.parse::<SymbolNodeId>().unwrap(),
                file_id.parse::<FileNodeId>().unwrap(),
                name,
            )
        })
        .collect())
}

/// Get all active symbol summaries (ID, file ID, name, kind, body_hash).
/// body_hash is now a dedicated, indexed column.
pub fn all_symbols_summary(store: &SqliteGraphStore) -> crate::Result<Vec<SymbolSummary>> {
    let conn = store.conn.lock();
    let mut stmt = conn.prepare_cached(
        "SELECT id, file_id, qualified_name, kind, body_hash FROM symbols WHERE retired_at_rev IS NULL ORDER BY id",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
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
                sym_id.parse::<SymbolNodeId>().unwrap(),
                file_id.parse::<FileNodeId>().unwrap(),
                qname,
                kind,
                body_hash,
            )
        })
        .collect())
}
