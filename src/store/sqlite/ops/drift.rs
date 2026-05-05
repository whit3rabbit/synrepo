//! Drift score and fingerprint operations for SqliteGraphStore.

use std::collections::HashMap;

use crate::{
    core::ids::{EdgeId, FileNodeId},
    structure::drift::StructuralFingerprint,
};

use super::super::SqliteGraphStore;

use rusqlite::params;

/// Get the latest revision for which drift scores exist.
pub fn latest_drift_revision(store: &SqliteGraphStore) -> crate::Result<Option<String>> {
    let conn = store.conn.lock();
    let mut stmt = conn.prepare_cached("SELECT MAX(revision) FROM edge_drift")?;
    let result: Option<String> = stmt.query_row([], |row| row.get(0)).ok().flatten();
    Ok(result)
}

/// Average drift score for the latest drift revision.
pub fn latest_drift_average(store: &SqliteGraphStore) -> crate::Result<Option<f32>> {
    let conn = store.conn.lock();
    let mut stmt = conn.prepare_cached(
        "SELECT AVG(drift_score) FROM edge_drift
         WHERE revision = (SELECT MAX(revision) FROM edge_drift)",
    )?;
    let result: Option<f32> = stmt.query_row([], |row| row.get(0)).ok().flatten();
    Ok(result)
}

/// Batch-write drift scores for a given revision.
pub fn write_drift_scores(
    store: &mut SqliteGraphStore,
    edge_scores: &[(EdgeId, f32)],
    revision: &str,
) -> crate::Result<()> {
    // Why: rusqlite Transaction RAII rolls back on drop. The previous manual
    // BEGIN/COMMIT via execute_batch leaked an open transaction on the
    // connection if any per-row insert failed via `?`, breaking the next
    // caller that locked the same connection.
    let mut conn = store.conn.lock();
    let tx = conn.transaction()?;
    for (edge_id, score) in edge_scores {
        tx.execute(
            "INSERT INTO edge_drift (edge_id, revision, drift_score)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(edge_id, revision) DO UPDATE SET drift_score = excluded.drift_score",
            params![edge_id.to_string(), revision, score],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// Read all drift scores for a given revision.
pub fn read_drift_scores(
    store: &SqliteGraphStore,
    revision: &str,
) -> crate::Result<Vec<(EdgeId, f32)>> {
    let conn = store.conn.lock();
    let mut stmt = conn.prepare_cached(
        "SELECT edge_id, drift_score FROM edge_drift WHERE revision = ?1 ORDER BY edge_id",
    )?;
    let rows = stmt
        .query_map(params![revision], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, f32>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut out = Vec::with_capacity(rows.len());
    for (id, score) in rows {
        let edge_id = id.parse::<EdgeId>().map_err(|e| {
            crate::Error::Other(anyhow::anyhow!(
                "invalid edge_id in edge_drift row {id:?}: {e}"
            ))
        })?;
        out.push((edge_id, score));
    }
    Ok(out)
}

/// Delete drift scores older than the given revision.
pub fn truncate_drift_scores(
    store: &SqliteGraphStore,
    older_than_revision: &str,
) -> crate::Result<usize> {
    let count = store.conn.lock().execute(
        "DELETE FROM edge_drift WHERE revision < ?1",
        params![older_than_revision],
    )?;
    Ok(count)
}

/// Check if any drift scores exist.
pub fn has_any_drift_scores(store: &SqliteGraphStore) -> crate::Result<bool> {
    let conn = store.conn.lock();
    let count: i64 = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM edge_drift LIMIT 1)",
        [],
        |row| row.get(0),
    )?;
    Ok(count != 0)
}

/// Get the latest revision for which fingerprints exist.
pub fn latest_fingerprint_revision(store: &SqliteGraphStore) -> crate::Result<Option<String>> {
    let conn = store.conn.lock();
    let mut stmt = conn.prepare_cached("SELECT MAX(revision) FROM file_fingerprints")?;
    let result: Option<String> = stmt.query_row([], |row| row.get(0)).ok().flatten();
    Ok(result)
}

/// Batch-write file fingerprints for a given revision.
pub fn write_fingerprints(
    store: &mut SqliteGraphStore,
    fingerprints: &[(FileNodeId, StructuralFingerprint)],
    revision: &str,
) -> crate::Result<()> {
    // Why: see write_drift_scores; same transaction-leak hazard.
    let mut conn = store.conn.lock();
    let tx = conn.transaction()?;
    for (file_id, fp) in fingerprints {
        let data = serde_json::to_string(fp).map_err(|e| anyhow::anyhow!(e))?;
        tx.execute(
            "INSERT INTO file_fingerprints (file_node_id, revision, fingerprint)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(file_node_id, revision) DO UPDATE SET fingerprint = excluded.fingerprint",
            params![file_id.to_string(), revision, data],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// Read all fingerprints for a given revision.
pub fn read_fingerprints(
    store: &SqliteGraphStore,
    revision: &str,
) -> crate::Result<HashMap<FileNodeId, StructuralFingerprint>> {
    let conn = store.conn.lock();
    let mut stmt = conn.prepare_cached(
        "SELECT file_node_id, fingerprint FROM file_fingerprints WHERE revision = ?1",
    )?;
    let rows = stmt
        .query_map(params![revision], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut map = HashMap::new();
    for (id, data) in rows {
        let fp: StructuralFingerprint =
            serde_json::from_str(&data).map_err(|e| anyhow::anyhow!(e))?;
        let file_id = id.parse::<FileNodeId>().map_err(|e| {
            crate::Error::Other(anyhow::anyhow!(
                "invalid file_node_id in file_fingerprints row {id:?}: {e}"
            ))
        })?;
        map.insert(file_id, fp);
    }
    Ok(map)
}

/// Delete fingerprints older than the given revision.
pub fn truncate_fingerprints(
    store: &SqliteGraphStore,
    older_than_revision: &str,
) -> crate::Result<usize> {
    let count = store.conn.lock().execute(
        "DELETE FROM file_fingerprints WHERE revision < ?1",
        params![older_than_revision],
    )?;
    Ok(count)
}
