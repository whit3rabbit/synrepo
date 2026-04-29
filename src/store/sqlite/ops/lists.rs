//! List query operations for SqliteGraphStore.

use crate::core::ids::{ConceptNodeId, FileNodeId, SymbolNodeId};
use crate::structure::graph::{SymbolKind, Visibility};

use super::super::SqliteGraphStore;

/// Return type for all_symbols_summary.
type SymbolSummary = (SymbolNodeId, FileNodeId, String, String, String);

/// Return type for all_symbols_for_resolution.
type SymbolForResolution = (
    SymbolNodeId,
    FileNodeId,
    String,
    SymbolKind,
    Visibility,
    String,
);

fn parse_graph_id<T>(value: &str, column: &str) -> crate::Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    value.parse::<T>().map_err(|error| {
        crate::Error::Other(anyhow::anyhow!(
            "invalid graph store {column} `{value}`: {error}"
        ))
    })
}

/// Get all file paths and their IDs, ordered by path.
pub fn all_file_paths(store: &SqliteGraphStore) -> crate::Result<Vec<(String, FileNodeId)>> {
    let conn = store.conn.lock();
    let mut stmt = conn.prepare_cached("SELECT path, id FROM files ORDER BY root_id, path")?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|(p, id)| Ok((p, parse_graph_id::<FileNodeId>(&id, "files.id")?)))
        .collect()
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
    rows.into_iter()
        .map(|(p, id)| Ok((p, parse_graph_id::<ConceptNodeId>(&id, "concepts.id")?)))
        .collect()
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
    rows.into_iter()
        .map(|(sym_id, file_id, name)| {
            Ok((
                parse_graph_id::<SymbolNodeId>(&sym_id, "symbols.id")?,
                parse_graph_id::<FileNodeId>(&file_id, "symbols.file_id")?,
                name,
            ))
        })
        .collect()
}

/// Get all active symbols with the fields stage-4 resolution needs
/// (ID, file ID, qualified name, kind, visibility, body hash).
///
/// `kind` is read from its dedicated column; `visibility` is deserialized
/// from a thin slice of the `data` JSON blob (one allocation per row,
/// avoids parsing the full `SymbolNode`).
pub fn all_symbols_for_resolution(
    store: &SqliteGraphStore,
) -> crate::Result<Vec<SymbolForResolution>> {
    #[derive(serde::Deserialize)]
    struct VisibilitySlice {
        #[serde(default)]
        visibility: Visibility,
    }

    let conn = store.conn.lock();
    let mut stmt = conn.prepare_cached(
        "SELECT id, file_id, qualified_name, kind, data, body_hash FROM symbols WHERE retired_at_rev IS NULL ORDER BY id",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut out = Vec::with_capacity(rows.len());
    for (sym_id, file_id, qname, kind_label, data, body_hash) in rows {
        let Some(kind) = SymbolKind::from_label(&kind_label) else {
            // Why: an unknown kind label means schema/binary drift between
            // when the row was written and the running binary. Surface it
            // so operators can investigate, then skip rather than fail the
            // entire query for one rotten row.
            tracing::warn!(
                symbol_id = %sym_id,
                kind_label = %kind_label,
                "skipping symbol with unknown SymbolKind label"
            );
            continue;
        };
        let visibility = serde_json::from_str::<VisibilitySlice>(&data)
            .map(|s| s.visibility)
            .unwrap_or_default();
        out.push((
            parse_graph_id::<SymbolNodeId>(&sym_id, "symbols.id")?,
            parse_graph_id::<FileNodeId>(&file_id, "symbols.file_id")?,
            qname,
            kind,
            visibility,
            body_hash,
        ));
    }
    Ok(out)
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
    rows.into_iter()
        .map(|(sym_id, file_id, qname, kind, body_hash)| {
            Ok((
                parse_graph_id::<SymbolNodeId>(&sym_id, "symbols.id")?,
                parse_graph_id::<FileNodeId>(&file_id, "symbols.file_id")?,
                qname,
                kind,
                body_hash,
            ))
        })
        .collect()
}
