//! Sqlite-backed canonical graph store.

use parking_lot::Mutex;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};
use std::{collections::BTreeMap, fs, path::Path};

use crate::{
    core::ids::{ConceptNodeId, FileNodeId, NodeId, SymbolNodeId},
    structure::graph::{ConceptNode, Edge, EdgeKind, FileNode, GraphStore, SymbolNode},
};

const GRAPH_DB_FILENAME: &str = "nodes.db";

/// Deterministic persisted graph statistics for the CLI surface.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PersistedGraphStats {
    /// Count of persisted file nodes.
    pub file_nodes: usize,
    /// Count of persisted symbol nodes.
    pub symbol_nodes: usize,
    /// Count of persisted concept nodes.
    pub concept_nodes: usize,
    /// Count of persisted edges across all kinds.
    pub total_edges: usize,
    /// Persisted edge counts keyed by stored edge kind label.
    pub edge_counts_by_kind: BTreeMap<String, usize>,
}

/// Sqlite-backed graph store rooted at `.synrepo/graph/`.
pub struct SqliteGraphStore {
    conn: Mutex<Connection>,
}

impl SqliteGraphStore {
    /// Open or create the canonical graph store inside `.synrepo/graph/`.
    pub fn open(graph_dir: &Path) -> crate::Result<Self> {
        fs::create_dir_all(graph_dir)?;
        Self::open_db(&graph_dir.join(GRAPH_DB_FILENAME))
    }

    /// Open or create the graph store at an explicit sqlite database path.
    pub fn open_db(db_path: &Path) -> crate::Result<Self> {
        if let Some(parent) = db_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(db_path)?;
        init_schema(&conn)?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Open an existing graph store without creating a new database.
    pub fn open_existing(graph_dir: &Path) -> crate::Result<Self> {
        let db_path = Self::db_path(graph_dir);
        if !db_path.exists() {
            return Err(crate::Error::Other(anyhow::anyhow!(
                "graph store is not materialized at {}",
                db_path.display()
            )));
        }

        let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        init_schema(&conn)?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Absolute path of the sqlite file used by the canonical graph store.
    pub fn db_path(graph_dir: &Path) -> std::path::PathBuf {
        graph_dir.join(GRAPH_DB_FILENAME)
    }

    /// Return deterministic persisted counts for the Phase 1 graph CLI.
    pub fn persisted_stats(&self) -> crate::Result<PersistedGraphStats> {
        let conn = self.conn.lock();
        let file_nodes = count_rows(&conn, "files")?;
        let symbol_nodes = count_rows(&conn, "symbols")?;
        let concept_nodes = count_rows(&conn, "concepts")?;
        let total_edges = count_rows(&conn, "edges")?;

        let mut stmt =
            conn.prepare("SELECT kind, COUNT(*) FROM edges GROUP BY kind ORDER BY kind")?;
        let counts = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, usize>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(PersistedGraphStats {
            file_nodes,
            symbol_nodes,
            concept_nodes,
            total_edges,
            edge_counts_by_kind: counts.into_iter().collect(),
        })
    }
}

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

    fn commit(&mut self) -> crate::Result<()> {
        // The first graph store slice uses sqlite's default autocommit mode.
        Ok(())
    }
}

fn init_schema(conn: &Connection) -> crate::Result<()> {
    conn.execute_batch(
        "
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS files (
            id INTEGER PRIMARY KEY,
            path TEXT NOT NULL UNIQUE,
            data TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS symbols (
            id INTEGER PRIMARY KEY,
            file_id INTEGER NOT NULL,
            qualified_name TEXT NOT NULL,
            kind TEXT NOT NULL,
            data TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_symbols_file_id ON symbols(file_id);

        CREATE TABLE IF NOT EXISTS concepts (
            id INTEGER PRIMARY KEY,
            path TEXT NOT NULL UNIQUE,
            data TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS edges (
            id INTEGER PRIMARY KEY,
            from_node_id TEXT NOT NULL,
            to_node_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            data TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_edges_from_kind ON edges(from_node_id, kind);
        CREATE INDEX IF NOT EXISTS idx_edges_to_kind ON edges(to_node_id, kind);
        ",
    )?;
    Ok(())
}

fn delete_node_inner(conn: &Connection, id: NodeId) -> crate::Result<()> {
    match id {
        NodeId::File(file_id) => {
            // TODO(phase-1): batch symbol edge deletion with DELETE ... WHERE id IN (subquery)
            // to avoid O(N) statements on files with many symbols.
            let mut stmt = conn.prepare("SELECT id FROM symbols WHERE file_id = ?1 ORDER BY id")?;
            let symbol_ids = stmt
                .query_map(params![file_id.0 as i64], |row| {
                    Ok(SymbolNodeId(row.get::<_, u64>(0)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;

            for symbol_id in symbol_ids {
                delete_node_inner(conn, NodeId::Symbol(symbol_id))?;
            }

            delete_edges_for(conn, id)?;
            conn.execute("DELETE FROM files WHERE id = ?1", params![file_id.0 as i64])?;
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

fn load_row<T, P>(conn: &Connection, sql: &str, params: P) -> crate::Result<Option<T>>
where
    T: DeserializeOwned,
    P: rusqlite::Params,
{
    conn.query_row(sql, params, |row| row.get::<_, String>(0))
        .optional()?
        .map(|json| decode_json(&json))
        .transpose()
}

fn load_rows<T, P>(conn: &Connection, sql: &str, params: P) -> crate::Result<Vec<T>>
where
    T: DeserializeOwned,
    P: rusqlite::Params,
{
    // TODO(phase-1): switch to prepare_cached once Connection is held in a CachedConnection wrapper.
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt
        .query_map(params, |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;

    rows.into_iter().map(|json| decode_json(&json)).collect()
}

fn count_rows(conn: &Connection, table: &str) -> crate::Result<usize> {
    let sql = format!("SELECT COUNT(*) FROM {table}");
    Ok(conn.query_row(&sql, [], |row| row.get::<_, usize>(0))?)
}

fn encode_json<T: Serialize>(value: &T) -> crate::Result<String> {
    serde_json::to_string(value).map_err(|error| {
        crate::Error::Other(anyhow::anyhow!("failed to encode graph row: {error}"))
    })
}

fn decode_json<T: DeserializeOwned>(json: &str) -> crate::Result<T> {
    serde_json::from_str(json).map_err(|error| {
        crate::Error::Other(anyhow::anyhow!("failed to decode graph row: {error}"))
    })
}

fn encode_label<T: Serialize>(value: &T) -> crate::Result<String> {
    let json = serde_json::to_value(value).map_err(|error| {
        crate::Error::Other(anyhow::anyhow!("failed to encode graph label: {error}"))
    })?;

    json.as_str().map(ToOwned::to_owned).ok_or_else(|| {
        crate::Error::Other(anyhow::anyhow!("graph label did not serialize to a string"))
    })
}

#[cfg(test)]
mod tests {
    use super::{PersistedGraphStats, SqliteGraphStore};
    use crate::{
        core::{
            ids::{ConceptNodeId, EdgeId, FileNodeId, NodeId, SymbolNodeId},
            provenance::{Provenance, SourceRef},
        },
        structure::graph::{
            ConceptNode, Edge, EdgeKind, Epistemic, FileNode, GraphStore, SymbolKind, SymbolNode,
        },
    };
    use std::collections::BTreeMap;
    use tempfile::tempdir;
    use time::OffsetDateTime;

    #[test]
    fn graph_store_round_trips_nodes_edges_and_provenance() {
        let repo = tempdir().unwrap();
        let graph_dir = repo.path().join(".synrepo/graph");
        let mut store = SqliteGraphStore::open(&graph_dir).unwrap();

        let file = FileNode {
            id: FileNodeId(0x42),
            path: "src/lib.rs".to_string(),
            path_history: vec!["src/old_lib.rs".to_string()],
            content_hash: "abc123".to_string(),
            size_bytes: 128,
            language: Some("rust".to_string()),
            epistemic: Epistemic::ParserObserved,
            provenance: sample_provenance("parse_code", "src/lib.rs"),
        };
        let symbol = SymbolNode {
            id: SymbolNodeId(0x24),
            file_id: file.id,
            qualified_name: "synrepo::lib".to_string(),
            display_name: "lib".to_string(),
            kind: SymbolKind::Module,
            body_byte_range: (0, 64),
            body_hash: "def456".to_string(),
            signature: Some("pub mod lib".to_string()),
            doc_comment: None,
            epistemic: Epistemic::ParserObserved,
            provenance: sample_provenance("parse_code", "src/lib.rs"),
        };
        let concept = ConceptNode {
            id: ConceptNodeId(0x99),
            path: "docs/adr/0001-graph.md".to_string(),
            title: "Graph Storage".to_string(),
            aliases: vec!["canonical-graph".to_string()],
            summary: Some("Why the graph stays observed-only.".to_string()),
            epistemic: Epistemic::HumanDeclared,
            provenance: sample_provenance("parse_prose", "docs/adr/0001-graph.md"),
        };
        let edge = Edge {
            id: EdgeId(0x77),
            from: NodeId::File(file.id),
            to: NodeId::Symbol(symbol.id),
            kind: EdgeKind::Defines,
            epistemic: Epistemic::ParserObserved,
            drift_score: 0.0,
            provenance: sample_provenance("resolve_edges", "src/lib.rs"),
        };

        store.upsert_file(file.clone()).unwrap();
        store.upsert_symbol(symbol.clone()).unwrap();
        store.upsert_concept(concept.clone()).unwrap();
        store.insert_edge(edge.clone()).unwrap();
        store.commit().unwrap();

        let loaded_file = store.get_file(file.id).unwrap().unwrap();
        let loaded_symbol = store.get_symbol(symbol.id).unwrap().unwrap();
        let loaded_concept = store.get_concept(concept.id).unwrap().unwrap();
        let outbound = store.outbound(NodeId::File(file.id), None).unwrap();

        assert_eq!(loaded_file.path, file.path);
        assert_eq!(loaded_file.path_history, file.path_history);
        assert_eq!(loaded_file.provenance.pass, "parse_code");
        assert_eq!(loaded_symbol.qualified_name, symbol.qualified_name);
        assert_eq!(loaded_symbol.body_hash, symbol.body_hash);
        assert_eq!(loaded_concept.title, concept.title);
        assert_eq!(loaded_concept.epistemic, Epistemic::HumanDeclared);
        assert_eq!(outbound.len(), 1);
        assert_eq!(outbound[0].kind, EdgeKind::Defines);
        assert_eq!(outbound[0].to, NodeId::Symbol(symbol.id));
        assert_eq!(
            store.file_by_path("src/lib.rs").unwrap().unwrap().id,
            FileNodeId(0x42)
        );
        assert!(SqliteGraphStore::db_path(&graph_dir).exists());
    }

    #[test]
    fn deleting_a_file_removes_child_symbols_and_incident_edges() {
        let repo = tempdir().unwrap();
        let graph_dir = repo.path().join(".synrepo/graph");
        let mut store = SqliteGraphStore::open(&graph_dir).unwrap();

        let file = FileNode {
            id: FileNodeId(1),
            path: "src/main.rs".to_string(),
            path_history: Vec::new(),
            content_hash: "main".to_string(),
            size_bytes: 10,
            language: Some("rust".to_string()),
            epistemic: Epistemic::ParserObserved,
            provenance: sample_provenance("parse_code", "src/main.rs"),
        };
        let symbol = SymbolNode {
            id: SymbolNodeId(2),
            file_id: file.id,
            qualified_name: "main".to_string(),
            display_name: "main".to_string(),
            kind: SymbolKind::Function,
            body_byte_range: (0, 10),
            body_hash: "body".to_string(),
            signature: Some("fn main()".to_string()),
            doc_comment: None,
            epistemic: Epistemic::ParserObserved,
            provenance: sample_provenance("parse_code", "src/main.rs"),
        };
        let edge = Edge {
            id: EdgeId(3),
            from: NodeId::File(file.id),
            to: NodeId::Symbol(symbol.id),
            kind: EdgeKind::Defines,
            epistemic: Epistemic::ParserObserved,
            drift_score: 0.0,
            provenance: sample_provenance("resolve_edges", "src/main.rs"),
        };

        store.upsert_file(file).unwrap();
        store.upsert_symbol(symbol).unwrap();
        store.insert_edge(edge).unwrap();

        store.delete_node(NodeId::File(FileNodeId(1))).unwrap();

        assert!(store.get_file(FileNodeId(1)).unwrap().is_none());
        assert!(store.get_symbol(SymbolNodeId(2)).unwrap().is_none());
        assert!(store
            .outbound(NodeId::File(FileNodeId(1)), None)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn open_existing_requires_materialized_graph_store() {
        let repo = tempdir().unwrap();
        let error = SqliteGraphStore::open_existing(&repo.path().join(".synrepo/graph"))
            .err()
            .unwrap()
            .to_string();

        assert!(error.contains("graph store is not materialized"));
    }

    #[test]
    fn persisted_stats_count_nodes_and_edges_by_kind() {
        let repo = tempdir().unwrap();
        let graph_dir = repo.path().join(".synrepo/graph");
        let mut store = SqliteGraphStore::open(&graph_dir).unwrap();

        let file = FileNode {
            id: FileNodeId(10),
            path: "src/lib.rs".to_string(),
            path_history: Vec::new(),
            content_hash: "a".to_string(),
            size_bytes: 1,
            language: Some("rust".to_string()),
            epistemic: Epistemic::ParserObserved,
            provenance: sample_provenance("parse_code", "src/lib.rs"),
        };
        let symbol = SymbolNode {
            id: SymbolNodeId(11),
            file_id: file.id,
            qualified_name: "crate::lib".to_string(),
            display_name: "lib".to_string(),
            kind: SymbolKind::Module,
            body_byte_range: (0, 1),
            body_hash: "b".to_string(),
            signature: None,
            doc_comment: None,
            epistemic: Epistemic::ParserObserved,
            provenance: sample_provenance("parse_code", "src/lib.rs"),
        };
        let concept = ConceptNode {
            id: ConceptNodeId(12),
            path: "docs/adr/0001.md".to_string(),
            title: "Decision".to_string(),
            aliases: Vec::new(),
            summary: None,
            epistemic: Epistemic::HumanDeclared,
            provenance: sample_provenance("parse_prose", "docs/adr/0001.md"),
        };

        store.upsert_file(file.clone()).unwrap();
        store.upsert_symbol(symbol.clone()).unwrap();
        store.upsert_concept(concept).unwrap();
        store
            .insert_edge(Edge {
                id: EdgeId(13),
                from: NodeId::File(file.id),
                to: NodeId::Symbol(symbol.id),
                kind: EdgeKind::Defines,
                epistemic: Epistemic::ParserObserved,
                drift_score: 0.0,
                provenance: sample_provenance("resolve_edges", "src/lib.rs"),
            })
            .unwrap();
        store
            .insert_edge(Edge {
                id: EdgeId(14),
                from: NodeId::Symbol(symbol.id),
                to: NodeId::File(file.id),
                kind: EdgeKind::References,
                epistemic: Epistemic::ParserObserved,
                drift_score: 0.0,
                provenance: sample_provenance("resolve_edges", "src/lib.rs"),
            })
            .unwrap();

        let stats = store.persisted_stats().unwrap();

        assert_eq!(
            stats,
            PersistedGraphStats {
                file_nodes: 1,
                symbol_nodes: 1,
                concept_nodes: 1,
                total_edges: 2,
                edge_counts_by_kind: BTreeMap::from([
                    ("defines".to_string(), 1),
                    ("references".to_string(), 1),
                ]),
            }
        );
    }

    fn sample_provenance(pass: &str, path: &str) -> Provenance {
        Provenance {
            created_at: OffsetDateTime::UNIX_EPOCH,
            source_revision: "deadbeef".to_string(),
            created_by: crate::core::provenance::CreatedBy::StructuralPipeline,
            pass: pass.to_string(),
            source_artifacts: vec![SourceRef {
                file_id: None,
                path: path.to_string(),
                content_hash: "hash".to_string(),
            }],
        }
    }
}
