//! Observation lifecycle: compile revisions, retirement, compaction.
//!
//! Split from `ops.rs` to keep that file under the 400-line limit.
//! These are inherent methods on `SqliteGraphStore`; the `GraphStore`
//! trait impl in `ops.rs` delegates to them.

use rusqlite::params;

use crate::{
    core::ids::{EdgeId, FileNodeId, SymbolNodeId},
    structure::graph::{CompactionSummary, Edge, SymbolNode},
};

use super::{codec::load_rows, SqliteGraphStore};

impl SqliteGraphStore {
    /// Allocate and return the next compile revision id.
    pub fn next_compile_revision_impl(&mut self) -> crate::Result<u64> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO compile_revisions (created_at) VALUES (datetime('now'))",
            [],
        )?;
        let rev: i64 = conn.query_row("SELECT last_insert_rowid()", [], |row| row.get(0))?;
        Ok(rev as u64)
    }

    /// Mark a symbol as retired at the given compile revision.
    pub fn retire_symbol_impl(&mut self, id: SymbolNodeId, revision: u64) -> crate::Result<()> {
        let conn = self.conn.lock();
        let rev = revision as i64;
        conn.execute(
            "UPDATE symbols SET retired_at_rev = ?2 WHERE id = ?1",
            params![id.0 as i64, rev],
        )?;
        conn.execute(
            "UPDATE symbols SET data = json_set(data, '$.retired_at_rev', ?2) WHERE id = ?1",
            params![id.0 as i64, rev],
        )?;
        Ok(())
    }

    /// Mark an edge as retired at the given compile revision.
    pub fn retire_edge_impl(&mut self, id: EdgeId, revision: u64) -> crate::Result<()> {
        let conn = self.conn.lock();
        let rev = revision as i64;
        conn.execute(
            "UPDATE edges SET retired_at_rev = ?2 WHERE id = ?1",
            params![id.0 as i64, rev],
        )?;
        conn.execute(
            "UPDATE edges SET data = json_set(data, '$.retired_at_rev', ?2) WHERE id = ?1",
            params![id.0 as i64, rev],
        )?;
        Ok(())
    }

    /// Clear retirement on a symbol and advance its last_observed_rev.
    pub fn unretire_symbol_impl(&mut self, id: SymbolNodeId, revision: u64) -> crate::Result<()> {
        let conn = self.conn.lock();
        let rev = revision as i64;
        conn.execute(
            "UPDATE symbols SET retired_at_rev = NULL, last_observed_rev = ?2 WHERE id = ?1",
            params![id.0 as i64, rev],
        )?;
        // In the JSON blob, remove retired_at_rev and set last_observed_rev.
        // json_set on a null key just sets it; json_remove on a missing key is a no-op.
        conn.execute(
            "UPDATE symbols SET data = json_set(json_remove(data, '$.retired_at_rev'), '$.last_observed_rev', ?2) WHERE id = ?1",
            params![id.0 as i64, rev],
        )?;
        Ok(())
    }

    /// Clear retirement on an edge and advance its last_observed_rev.
    pub fn unretire_edge_impl(&mut self, id: EdgeId, revision: u64) -> crate::Result<()> {
        let conn = self.conn.lock();
        let rev = revision as i64;
        conn.execute(
            "UPDATE edges SET retired_at_rev = NULL, last_observed_rev = ?2 WHERE id = ?1",
            params![id.0 as i64, rev],
        )?;
        conn.execute(
            "UPDATE edges SET data = json_set(json_remove(data, '$.retired_at_rev'), '$.last_observed_rev', ?2) WHERE id = ?1",
            params![id.0 as i64, rev],
        )?;
        Ok(())
    }

    /// Return all active (non-retired) symbols owned by a file.
    pub fn symbols_for_file_impl(&self, file_id: FileNodeId) -> crate::Result<Vec<SymbolNode>> {
        let conn = self.conn.lock();
        load_rows(
            &conn,
            "SELECT data FROM symbols WHERE file_id = ?1 AND retired_at_rev IS NULL ORDER BY id",
            params![file_id.0 as i64],
        )
    }

    /// Return all active (non-retired) edges owned by a file.
    pub fn edges_owned_by_impl(&self, file_id: FileNodeId) -> crate::Result<Vec<Edge>> {
        let conn = self.conn.lock();
        load_rows(
            &conn,
            "SELECT data FROM edges WHERE owner_file_id = ?1 AND retired_at_rev IS NULL ORDER BY id",
            params![file_id.0 as i64],
        )
    }

    /// Return all active (non-retired) edges.
    pub fn active_edges_impl(&self) -> crate::Result<Vec<Edge>> {
        let conn = self.conn.lock();
        load_rows(
            &conn,
            "SELECT data FROM edges WHERE retired_at_rev IS NULL ORDER BY id",
            params![],
        )
    }

    /// Physically delete retired observations older than `older_than_rev`.
    pub fn compact_retired_impl(
        &mut self,
        older_than_rev: u64,
    ) -> crate::Result<CompactionSummary> {
        let conn = self.conn.lock();
        let rev = older_than_rev as i64;

        let symbols_removed = conn.execute(
            "DELETE FROM symbols WHERE retired_at_rev IS NOT NULL AND retired_at_rev < ?1",
            params![rev],
        )?;
        let edges_removed = conn.execute(
            "DELETE FROM edges WHERE retired_at_rev IS NOT NULL AND retired_at_rev < ?1",
            params![rev],
        )?;
        let revisions_removed = conn.execute(
            "DELETE FROM compile_revisions WHERE revision_id < ?1",
            params![rev],
        )?;

        // Prune old sidecar data keyed by revision string.
        let rev_str = older_than_rev.to_string();
        conn.execute(
            "DELETE FROM edge_drift WHERE revision < ?1",
            params![rev_str],
        )?;
        conn.execute(
            "DELETE FROM file_fingerprints WHERE revision < ?1",
            params![rev_str],
        )?;

        Ok(CompactionSummary {
            symbols_removed,
            edges_removed,
            revisions_removed,
        })
    }
}
