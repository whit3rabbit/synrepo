use std::collections::HashMap;

use rusqlite::{params, Connection};

use crate::{
    core::ids::{ConceptNodeId, EdgeId, FileNodeId, NodeId, SymbolNodeId},
    structure::drift::StructuralFingerprint,
    structure::graph::{ConceptNode, Edge, EdgeKind, FileNode, GraphStore, SymbolNode},
};

use super::{
    codec::{encode_json, encode_label, load_row, load_rows},
    SqliteGraphStore,
};

impl GraphStore for SqliteGraphStore {
    fn upsert_file(&mut self, node: FileNode) -> crate::Result<()> {
        let data = encode_json(&node)?;
        let id = node.id.0 as i64;
        let path = node.path;
        let last_obs = node.last_observed_rev.map(|r| r as i64);

        self.conn.lock().execute(
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

    fn upsert_symbol(&mut self, node: SymbolNode) -> crate::Result<()> {
        let data = encode_json(&node)?;
        let kind = encode_label(&node.kind)?;
        let id = node.id.0 as i64;
        let file_id = node.file_id.0 as i64;
        let qualified_name = node.qualified_name;
        let first_seen_rev = &node.first_seen_rev;
        let last_modified_rev = &node.last_modified_rev;
        let last_obs = node.last_observed_rev.map(|r| r as i64);
        let retired = node.retired_at_rev.map(|r| r as i64);

        self.conn.lock().execute(
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

    fn upsert_concept(&mut self, node: ConceptNode) -> crate::Result<()> {
        let data = encode_json(&node)?;
        let id = node.id.0 as i64;
        let path = node.path;
        let last_obs = node.last_observed_rev.map(|r| r as i64);

        self.conn.lock().execute(
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

    fn insert_edge(&mut self, edge: Edge) -> crate::Result<()> {
        let data = encode_json(&edge)?;
        let kind = encode_label(&edge.kind)?;
        let id = edge.id.0 as i64;
        let from_node_id = edge.from.to_string();
        let to_node_id = edge.to.to_string();
        let owner_fid = edge.owner_file_id.map(|fid| fid.0 as i64);
        let last_obs = edge.last_observed_rev.map(|r| r as i64);
        let retired = edge.retired_at_rev.map(|r| r as i64);

        self.conn.lock().execute(
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

    fn delete_edge(&mut self, edge_id: EdgeId) -> crate::Result<()> {
        let id = edge_id.0 as i64;
        self.conn
            .lock()
            .execute("DELETE FROM edges WHERE id = ?1", params![id])?;
        Ok(())
    }

    fn delete_edges_by_kind(&mut self, kind: EdgeKind) -> crate::Result<usize> {
        let kind_label = encode_label(&kind)?;
        let count = self
            .conn
            .lock()
            .execute("DELETE FROM edges WHERE kind = ?1", params![kind_label])?;
        Ok(count)
    }

    fn delete_node(&mut self, id: NodeId) -> crate::Result<()> {
        let conn = self.conn.lock();
        delete_node_inner(&conn, id)
    }

    fn get_file(&self, id: FileNodeId) -> crate::Result<Option<FileNode>> {
        let conn = self.conn.lock();
        load_row(
            &conn,
            "SELECT data FROM files WHERE id = ?1",
            params![id.0 as i64],
        )
    }

    fn get_symbol(&self, id: SymbolNodeId) -> crate::Result<Option<SymbolNode>> {
        let conn = self.conn.lock();
        load_row(
            &conn,
            "SELECT data FROM symbols WHERE id = ?1",
            params![id.0 as i64],
        )
    }

    fn get_concept(&self, id: ConceptNodeId) -> crate::Result<Option<ConceptNode>> {
        let conn = self.conn.lock();
        load_row(
            &conn,
            "SELECT data FROM concepts WHERE id = ?1",
            params![id.0 as i64],
        )
    }

    fn file_by_path(&self, path: &str) -> crate::Result<Option<FileNode>> {
        let conn = self.conn.lock();
        load_row(
            &conn,
            "SELECT data FROM files WHERE path = ?1",
            params![path],
        )
    }

    fn outbound(&self, from: NodeId, kind: Option<EdgeKind>) -> crate::Result<Vec<Edge>> {
        let conn = self.conn.lock();
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

    fn inbound(&self, to: NodeId, kind: Option<EdgeKind>) -> crate::Result<Vec<Edge>> {
        let conn = self.conn.lock();
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

    fn begin(&mut self) -> crate::Result<()> {
        self.conn.lock().execute_batch("BEGIN DEFERRED")?;
        Ok(())
    }

    fn commit(&mut self) -> crate::Result<()> {
        self.conn.lock().execute_batch("COMMIT")?;
        Ok(())
    }

    fn rollback(&mut self) -> crate::Result<()> {
        self.conn.lock().execute_batch("ROLLBACK")?;
        Ok(())
    }

    fn begin_read_snapshot(&self) -> crate::Result<()> {
        // Re-entrant: only the outermost begin issues BEGIN DEFERRED. Inner
        // begins share the outer snapshot so callers composing wrapped
        // operations (e.g. an MCP handler calling GraphCardCompiler, which
        // wraps internally) don't trip SQLite's "transaction within a
        // transaction" error. The depth lock is taken across the SQL issue
        // to keep depth and transaction state consistent.
        let mut depth = self.snapshot_depth.lock();
        if *depth == 0 {
            self.conn.lock().execute_batch("BEGIN DEFERRED")?;
        }
        *depth += 1;
        Ok(())
    }

    fn end_read_snapshot(&self) -> crate::Result<()> {
        let mut depth = self.snapshot_depth.lock();
        if *depth == 0 {
            // end-without-begin: treat as no-op so the `with_*` helper's
            // error-path cleanup can't mask the caller's original error.
            return Ok(());
        }
        *depth -= 1;
        if *depth == 0 {
            let conn = self.conn.lock();
            match conn.execute_batch("COMMIT") {
                Ok(()) => Ok(()),
                Err(err) if err.to_string().contains("no transaction") => Ok(()),
                Err(err) => Err(err.into()),
            }
        } else {
            Ok(())
        }
    }

    fn all_file_paths(&self) -> crate::Result<Vec<(String, FileNodeId)>> {
        let conn = self.conn.lock();
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

    fn all_concept_paths(&self) -> crate::Result<Vec<(String, ConceptNodeId)>> {
        let conn = self.conn.lock();
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

    fn all_symbol_names(&self) -> crate::Result<Vec<(SymbolNodeId, FileNodeId, String)>> {
        let conn = self.conn.lock();
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

    fn all_symbols_summary(
        &self,
    ) -> crate::Result<Vec<(SymbolNodeId, FileNodeId, String, String, String)>> {
        let conn = self.conn.lock();
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

    fn latest_drift_revision(&self) -> crate::Result<Option<String>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare_cached("SELECT MAX(revision) FROM edge_drift")?;
        let result: Option<String> = stmt.query_row([], |row| row.get(0)).ok().flatten();
        Ok(result)
    }

    fn write_drift_scores(
        &mut self,
        scores: &[(EdgeId, f32)],
        revision: &str,
    ) -> crate::Result<()> {
        SqliteGraphStore::write_drift_scores(self, scores, revision)
    }

    fn read_drift_scores(&self, revision: &str) -> crate::Result<Vec<(EdgeId, f32)>> {
        SqliteGraphStore::read_drift_scores(self, revision)
    }

    fn truncate_drift_scores(&self, older_than_revision: &str) -> crate::Result<usize> {
        SqliteGraphStore::truncate_drift_scores(self, older_than_revision)
    }

    fn has_any_drift_scores(&self) -> crate::Result<bool> {
        let conn = self.conn.lock();
        let count: i64 = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM edge_drift LIMIT 1)",
            [],
            |row| row.get(0),
        )?;
        Ok(count != 0)
    }

    fn latest_fingerprint_revision(&self) -> crate::Result<Option<String>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare_cached("SELECT MAX(revision) FROM file_fingerprints")?;
        let result: Option<String> = stmt.query_row([], |row| row.get(0)).ok().flatten();
        Ok(result)
    }

    fn all_edges(&self) -> crate::Result<Vec<Edge>> {
        let conn = self.conn.lock();
        load_rows(
            &conn,
            "SELECT data FROM edges WHERE retired_at_rev IS NULL ORDER BY id",
            params![],
        )
    }

    fn write_fingerprints(
        &mut self,
        fingerprints: &[(FileNodeId, StructuralFingerprint)],
        revision: &str,
    ) -> crate::Result<()> {
        let conn = self.conn.lock();
        conn.execute_batch("BEGIN")?;
        for (file_id, fp) in fingerprints {
            let data = serde_json::to_string(fp).map_err(|e| anyhow::anyhow!(e))?;
            conn.execute(
                "INSERT INTO file_fingerprints (file_node_id, revision, fingerprint)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(file_node_id, revision) DO UPDATE SET fingerprint = excluded.fingerprint",
                params![file_id.0 as i64, revision, data],
            )?;
        }
        conn.execute_batch("COMMIT")?;
        Ok(())
    }

    fn read_fingerprints(
        &self,
        revision: &str,
    ) -> crate::Result<HashMap<FileNodeId, StructuralFingerprint>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare_cached(
            "SELECT file_node_id, fingerprint FROM file_fingerprints WHERE revision = ?1",
        )?;
        let rows = stmt
            .query_map(params![revision], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut map = HashMap::new();
        for (id, data) in rows {
            let fp: StructuralFingerprint =
                serde_json::from_str(&data).map_err(|e| anyhow::anyhow!(e))?;
            map.insert(FileNodeId(id as u64), fp);
        }
        Ok(map)
    }

    fn truncate_fingerprints(&self, older_than_revision: &str) -> crate::Result<usize> {
        let count = self.conn.lock().execute(
            "DELETE FROM file_fingerprints WHERE revision < ?1",
            params![older_than_revision],
        )?;
        Ok(count)
    }

    // -- Observation lifecycle (delegates to lifecycle.rs) ------------------

    fn next_compile_revision(&mut self) -> crate::Result<u64> {
        self.next_compile_revision_impl()
    }

    fn retire_symbol(
        &mut self,
        id: crate::core::ids::SymbolNodeId,
        revision: u64,
    ) -> crate::Result<()> {
        self.retire_symbol_impl(id, revision)
    }

    fn retire_edge(&mut self, id: EdgeId, revision: u64) -> crate::Result<()> {
        self.retire_edge_impl(id, revision)
    }

    fn unretire_symbol(
        &mut self,
        id: crate::core::ids::SymbolNodeId,
        revision: u64,
    ) -> crate::Result<()> {
        self.unretire_symbol_impl(id, revision)
    }

    fn unretire_edge(&mut self, id: EdgeId, revision: u64) -> crate::Result<()> {
        self.unretire_edge_impl(id, revision)
    }

    fn symbols_for_file(
        &self,
        file_id: FileNodeId,
    ) -> crate::Result<Vec<crate::structure::graph::SymbolNode>> {
        self.symbols_for_file_impl(file_id)
    }

    fn edges_owned_by(
        &self,
        file_id: FileNodeId,
    ) -> crate::Result<Vec<crate::structure::graph::Edge>> {
        self.edges_owned_by_impl(file_id)
    }

    fn active_edges(&self) -> crate::Result<Vec<crate::structure::graph::Edge>> {
        self.active_edges_impl()
    }

    fn compact_retired(
        &mut self,
        older_than_rev: u64,
    ) -> crate::Result<crate::structure::graph::CompactionSummary> {
        self.compact_retired_impl(older_than_rev)
    }
}

impl SqliteGraphStore {
    /// Batch-write drift scores for a given revision.
    pub fn write_drift_scores(
        &self,
        edge_scores: &[(EdgeId, f32)],
        revision: &str,
    ) -> crate::Result<()> {
        let conn = self.conn.lock();
        conn.execute_batch("BEGIN")?;
        for (edge_id, score) in edge_scores {
            conn.execute(
                "INSERT INTO edge_drift (edge_id, revision, drift_score)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(edge_id, revision) DO UPDATE SET drift_score = excluded.drift_score",
                params![edge_id.0 as i64, revision, score],
            )?;
        }
        conn.execute_batch("COMMIT")?;
        Ok(())
    }

    /// Read all drift scores for a given revision.
    pub fn read_drift_scores(&self, revision: &str) -> crate::Result<Vec<(EdgeId, f32)>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare_cached(
            "SELECT edge_id, drift_score FROM edge_drift WHERE revision = ?1 ORDER BY edge_id",
        )?;
        let rows = stmt
            .query_map(params![revision], |row| {
                Ok((row.get::<_, i64>(0)? as u64, row.get::<_, f32>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows
            .into_iter()
            .map(|(id, score)| (EdgeId(id), score))
            .collect())
    }

    /// Delete drift scores older than the given revision.
    pub fn truncate_drift_scores(&self, older_than_revision: &str) -> crate::Result<usize> {
        let count = self.conn.lock().execute(
            "DELETE FROM edge_drift WHERE revision < ?1",
            params![older_than_revision],
        )?;
        Ok(count)
    }
}

pub(super) fn delete_node_inner(conn: &Connection, id: NodeId) -> crate::Result<()> {
    match id {
        NodeId::File(file_id) => {
            let fid = file_id.0 as i64;
            // Batch-delete all edges incident to any symbol belonging to this file.
            // printf('sym_%016x', id) matches SymbolNodeId::to_string() for all u64 values:
            // SQLite treats the integer as unsigned for %x, matching Rust's {:016x}.
            conn.execute(
                "DELETE FROM edges
                 WHERE from_node_id IN (SELECT printf('sym_%016x', id) FROM symbols WHERE file_id = ?1)
                    OR to_node_id   IN (SELECT printf('sym_%016x', id) FROM symbols WHERE file_id = ?1)",
                params![fid],
            )?;
            conn.execute("DELETE FROM symbols WHERE file_id = ?1", params![fid])?;
            delete_edges_for(conn, id)?;
            conn.execute("DELETE FROM files WHERE id = ?1", params![fid])?;
        }
        NodeId::Symbol(symbol_id) => {
            delete_edges_for(conn, id)?;
            conn.execute(
                "DELETE FROM symbols WHERE id = ?1",
                params![symbol_id.0 as i64],
            )?;
        }
        NodeId::Concept(concept_id) => {
            delete_edges_for(conn, id)?;
            conn.execute(
                "DELETE FROM concepts WHERE id = ?1",
                params![concept_id.0 as i64],
            )?;
        }
    }

    Ok(())
}

fn delete_edges_for(conn: &Connection, id: NodeId) -> crate::Result<()> {
    let node_id = id.to_string();
    conn.execute(
        "DELETE FROM edges WHERE from_node_id = ?1 OR to_node_id = ?1",
        params![node_id],
    )?;
    Ok(())
}
