use rusqlite::{params, Connection};

use crate::{
    core::ids::{ConceptNodeId, FileNodeId, NodeId, SymbolNodeId},
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

        self.conn.lock().execute(
            "INSERT INTO files (id, path, data)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET path = excluded.path, data = excluded.data",
            params![id, path, data],
        )?;
        Ok(())
    }

    fn upsert_symbol(&mut self, node: SymbolNode) -> crate::Result<()> {
        let data = encode_json(&node)?;
        let kind = encode_label(&node.kind)?;
        let id = node.id.0 as i64;
        let file_id = node.file_id.0 as i64;
        let qualified_name = node.qualified_name;

        self.conn.lock().execute(
            "INSERT INTO symbols (id, file_id, qualified_name, kind, data)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
                 file_id = excluded.file_id,
                 qualified_name = excluded.qualified_name,
                 kind = excluded.kind,
                 data = excluded.data",
            params![id, file_id, qualified_name, kind, data],
        )?;
        Ok(())
    }

    fn upsert_concept(&mut self, node: ConceptNode) -> crate::Result<()> {
        let data = encode_json(&node)?;
        let id = node.id.0 as i64;
        let path = node.path;

        self.conn.lock().execute(
            "INSERT INTO concepts (id, path, data)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET path = excluded.path, data = excluded.data",
            params![id, path, data],
        )?;
        Ok(())
    }

    fn insert_edge(&mut self, edge: Edge) -> crate::Result<()> {
        let data = encode_json(&edge)?;
        let kind = encode_label(&edge.kind)?;
        let id = edge.id.0 as i64;
        let from_node_id = edge.from.to_string();
        let to_node_id = edge.to.to_string();

        self.conn.lock().execute(
            "INSERT INTO edges (id, from_node_id, to_node_id, kind, data)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
                 from_node_id = excluded.from_node_id,
                 to_node_id = excluded.to_node_id,
                 kind = excluded.kind,
                 data = excluded.data",
            params![id, from_node_id, to_node_id, kind, data],
        )?;
        Ok(())
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
                "SELECT data FROM edges WHERE from_node_id = ?1 AND kind = ?2 ORDER BY id",
                params![from_node_id, kind],
            )
        } else {
            load_rows(
                &conn,
                "SELECT data FROM edges WHERE from_node_id = ?1 ORDER BY id",
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
                "SELECT data FROM edges WHERE to_node_id = ?1 AND kind = ?2 ORDER BY id",
                params![to_node_id, kind],
            )
        } else {
            load_rows(
                &conn,
                "SELECT data FROM edges WHERE to_node_id = ?1 ORDER BY id",
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
        let mut stmt =
            conn.prepare_cached("SELECT id, file_id, qualified_name FROM symbols ORDER BY id")?;
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
