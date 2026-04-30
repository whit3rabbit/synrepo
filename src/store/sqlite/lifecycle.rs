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
            params![id.to_string(), rev],
        )?;
        conn.execute(
            "UPDATE symbols SET data = json_set(data, '$.retired_at_rev', ?2) WHERE id = ?1",
            params![id.to_string(), rev],
        )?;
        Ok(())
    }

    /// Mark an edge as retired at the given compile revision.
    pub fn retire_edge_impl(&mut self, id: EdgeId, revision: u64) -> crate::Result<()> {
        let conn = self.conn.lock();
        let rev = revision as i64;
        conn.execute(
            "UPDATE edges SET retired_at_rev = ?2 WHERE id = ?1",
            params![id.to_string(), rev],
        )?;
        conn.execute(
            "UPDATE edges SET data = json_set(data, '$.retired_at_rev', ?2) WHERE id = ?1",
            params![id.to_string(), rev],
        )?;
        Ok(())
    }

    /// Conservative cap on the number of bound parameters per chunked UPDATE.
    /// SQLite's default `SQLITE_MAX_VARIABLE_NUMBER` is 999 on older builds;
    /// 500 leaves headroom for the trailing revision parameter and any
    /// future additions to the SET clause.
    const RETIRE_BULK_CHUNK: usize = 500;

    /// Mark many symbols retired at `revision` in a single chunked UPDATE.
    pub fn retire_symbols_bulk_impl(
        &mut self,
        ids: &[SymbolNodeId],
        revision: u64,
    ) -> crate::Result<()> {
        self.retire_bulk_impl("symbols", ids, revision)
    }

    /// Mark many edges retired at `revision` in a single chunked UPDATE.
    pub fn retire_edges_bulk_impl(&mut self, ids: &[EdgeId], revision: u64) -> crate::Result<()> {
        self.retire_bulk_impl("edges", ids, revision)
    }

    /// Shared chunked UPDATE for retire-bulk on either `symbols` or `edges`.
    /// `table` is a `&'static str` so callers can only pass hardcoded table
    /// names; SQLite cannot bind identifiers, so injection is the trade-off.
    fn retire_bulk_impl<I: ToString>(
        &mut self,
        table: &'static str,
        ids: &[I],
        revision: u64,
    ) -> crate::Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        let conn = self.conn.lock();
        let rev = revision as i64;
        for chunk in ids.chunks(Self::RETIRE_BULK_CHUNK) {
            let placeholders: String = std::iter::repeat_n("?", chunk.len())
                .collect::<Vec<_>>()
                .join(",");
            let id_strings: Vec<String> = chunk.iter().map(|id| id.to_string()).collect();
            let sql = format!(
                "UPDATE {table} SET retired_at_rev = ?, \
                  data = json_set(data, '$.retired_at_rev', ?) \
                  WHERE id IN ({placeholders})"
            );
            let mut params_vec: Vec<rusqlite::types::Value> = vec![rev.into(), rev.into()];
            for id_string in &id_strings {
                params_vec.push(id_string.clone().into());
            }
            conn.execute(&sql, rusqlite::params_from_iter(params_vec.iter()))?;
        }
        Ok(())
    }

    /// Clear retirement on a symbol and advance its last_observed_rev.
    pub fn unretire_symbol_impl(&mut self, id: SymbolNodeId, revision: u64) -> crate::Result<()> {
        let conn = self.conn.lock();
        let rev = revision as i64;
        conn.execute(
            "UPDATE symbols SET retired_at_rev = NULL, last_observed_rev = ?2 WHERE id = ?1",
            params![id.to_string(), rev],
        )?;
        // In the JSON blob, remove retired_at_rev and set last_observed_rev.
        // json_set on a null key just sets it; json_remove on a missing key is a no-op.
        conn.execute(
            "UPDATE symbols SET data = json_set(json_remove(data, '$.retired_at_rev'), '$.last_observed_rev', ?2) WHERE id = ?1",
            params![id.to_string(), rev],
        )?;
        Ok(())
    }

    /// Clear retirement on an edge and advance its last_observed_rev.
    pub fn unretire_edge_impl(&mut self, id: EdgeId, revision: u64) -> crate::Result<()> {
        let conn = self.conn.lock();
        let rev = revision as i64;
        conn.execute(
            "UPDATE edges SET retired_at_rev = NULL, last_observed_rev = ?2 WHERE id = ?1",
            params![id.to_string(), rev],
        )?;
        conn.execute(
            "UPDATE edges SET data = json_set(json_remove(data, '$.retired_at_rev'), '$.last_observed_rev', ?2) WHERE id = ?1",
            params![id.to_string(), rev],
        )?;
        Ok(())
    }

    /// Return all active (non-retired) symbols owned by a file.
    pub fn symbols_for_file_impl(&self, file_id: FileNodeId) -> crate::Result<Vec<SymbolNode>> {
        let conn = self.conn.lock();
        load_rows(
            &conn,
            "SELECT data FROM symbols WHERE file_id = ?1 AND retired_at_rev IS NULL ORDER BY id",
            params![file_id.to_string()],
        )
    }

    /// Return all active (non-retired) edges owned by a file.
    pub fn edges_owned_by_impl(&self, file_id: FileNodeId) -> crate::Result<Vec<Edge>> {
        let conn = self.conn.lock();
        load_rows(
            &conn,
            "SELECT data FROM edges WHERE owner_file_id = ?1 AND retired_at_rev IS NULL ORDER BY id",
            params![file_id.to_string()],
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

        // Cascade delete edges that point to/from symbols we are about to delete
        let cascading_edges_removed = conn.execute(
            "DELETE FROM edges WHERE 
             from_node_id IN (SELECT id FROM symbols WHERE retired_at_rev IS NOT NULL AND retired_at_rev < ?1)
             OR to_node_id IN (SELECT id FROM symbols WHERE retired_at_rev IS NOT NULL AND retired_at_rev < ?1)",
            params![rev],
        )?;

        let symbols_removed = conn.execute(
            "DELETE FROM symbols WHERE retired_at_rev IS NOT NULL AND retired_at_rev < ?1",
            params![rev],
        )?;
        let explicit_edges_removed = conn.execute(
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
            edges_removed: explicit_edges_removed + cascading_edges_removed,
            revisions_removed,
        })
    }
}
