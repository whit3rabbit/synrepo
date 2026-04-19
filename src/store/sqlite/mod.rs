//! Sqlite-backed canonical graph store.

mod codec;
mod lifecycle;
mod ops;
mod schema;

#[cfg(test)]
mod tests;

use parking_lot::Mutex;
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use schema::{count_rows, init_schema};

use crate::core::ids::{ConceptNodeId, FileNodeId, NodeId, SymbolNodeId};
use crate::structure::graph::{ConceptNode, Edge, EdgeKind, FileNode, GraphReader, SymbolNode};

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
    pub(super) conn: Mutex<Connection>,
    /// Re-entrant read-snapshot depth counter. `begin_read_snapshot` issues
    /// `BEGIN DEFERRED` only on the 0 -> 1 transition; `end_read_snapshot`
    /// issues `COMMIT` only on the 1 -> 0 transition. This keeps nested
    /// snapshots safe (e.g. an MCP handler wraps its body, and an inner
    /// `GraphCardCompiler` method also wraps its body) while preserving the
    /// "single committed epoch for the whole scope" guarantee.
    pub(super) snapshot_depth: Mutex<usize>,
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
            snapshot_depth: Mutex::new(0),
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
            snapshot_depth: Mutex::new(0),
        })
    }

    /// Absolute path of the sqlite file used by the canonical graph store.
    pub fn db_path(graph_dir: &Path) -> PathBuf {
        graph_dir.join(GRAPH_DB_FILENAME)
    }

    /// Inherent shim for the read-only graph surface.
    pub fn get_file(&self, id: FileNodeId) -> crate::Result<Option<FileNode>> {
        GraphReader::get_file(self, id)
    }

    /// Inherent shim for the read-only graph surface.
    pub fn get_symbol(&self, id: SymbolNodeId) -> crate::Result<Option<SymbolNode>> {
        GraphReader::get_symbol(self, id)
    }

    /// Inherent shim for the read-only graph surface.
    pub fn get_concept(&self, id: ConceptNodeId) -> crate::Result<Option<ConceptNode>> {
        GraphReader::get_concept(self, id)
    }

    /// Inherent shim for the read-only graph surface.
    pub fn file_by_path(&self, path: &str) -> crate::Result<Option<FileNode>> {
        GraphReader::file_by_path(self, path)
    }

    /// Inherent shim for the read-only graph surface.
    pub fn outbound(&self, from: NodeId, kind: Option<EdgeKind>) -> crate::Result<Vec<Edge>> {
        GraphReader::outbound(self, from, kind)
    }

    /// Inherent shim for the read-only graph surface.
    pub fn inbound(&self, to: NodeId, kind: Option<EdgeKind>) -> crate::Result<Vec<Edge>> {
        GraphReader::inbound(self, to, kind)
    }

    /// Inherent shim for the read-only graph surface.
    pub fn all_file_paths(&self) -> crate::Result<Vec<(String, FileNodeId)>> {
        GraphReader::all_file_paths(self)
    }

    /// Inherent shim for the read-only graph surface.
    pub fn all_concept_paths(&self) -> crate::Result<Vec<(String, ConceptNodeId)>> {
        GraphReader::all_concept_paths(self)
    }

    /// Inherent shim for the read-only graph surface.
    pub fn all_symbol_names(&self) -> crate::Result<Vec<(SymbolNodeId, FileNodeId, String)>> {
        GraphReader::all_symbol_names(self)
    }

    /// Inherent shim for the read-only graph surface.
    #[allow(clippy::type_complexity)]
    pub fn all_symbols_summary(
        &self,
    ) -> crate::Result<Vec<(SymbolNodeId, FileNodeId, String, String, String)>> {
        GraphReader::all_symbols_summary(self)
    }

    /// Inherent shim for the read-only graph surface.
    pub fn symbols_for_file(&self, file_id: FileNodeId) -> crate::Result<Vec<SymbolNode>> {
        GraphReader::symbols_for_file(self, file_id)
    }

    /// Inherent shim for the read-only graph surface.
    pub fn edges_owned_by(&self, file_id: FileNodeId) -> crate::Result<Vec<Edge>> {
        GraphReader::edges_owned_by(self, file_id)
    }

    /// Inherent shim for the read-only graph surface.
    pub fn all_edges(&self) -> crate::Result<Vec<Edge>> {
        GraphReader::all_edges(self)
    }

    /// Inherent shim for the read-only graph surface.
    pub fn active_edges(&self) -> crate::Result<Vec<Edge>> {
        GraphReader::active_edges(self)
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
