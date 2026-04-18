//! Helper functions for SqliteGraphStore operations.

use rusqlite::{params, Connection};

use crate::core::ids::NodeId;

/// Delete a node and all associated edges.
pub fn delete_node_inner(conn: &Connection, id: NodeId) -> crate::Result<()> {
    match id {
        NodeId::File(file_id) => {
            let fid = file_id.to_string();
            // Batch-delete all edges incident to any symbol belonging to this file.
            conn.execute(
                "DELETE FROM edges
                 WHERE from_node_id IN (SELECT id FROM symbols WHERE file_id = ?1)
                    OR to_node_id   IN (SELECT id FROM symbols WHERE file_id = ?1)",
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
                params![symbol_id.to_string()],
            )?;
        }
        NodeId::Concept(concept_id) => {
            delete_edges_for(conn, id)?;
            conn.execute(
                "DELETE FROM concepts WHERE id = ?1",
                params![concept_id.to_string()],
            )?;
        }
    }

    Ok(())
}

/// Delete all edges incident to a node (both inbound and outbound).
fn delete_edges_for(conn: &Connection, id: NodeId) -> crate::Result<()> {
    let node_id = id.to_string();
    conn.execute(
        "DELETE FROM edges WHERE from_node_id = ?1 OR to_node_id = ?1",
        params![node_id],
    )?;
    Ok(())
}
