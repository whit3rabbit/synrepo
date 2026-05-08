use crate::overlay::{AgentNote, AgentNoteCounts, AgentNoteQuery, AgentNoteStatus};

use super::super::{sqlite_values::row_usize, SqliteOverlayStore};
use super::codec::note_from_row;

impl SqliteOverlayStore {
    /// Return note counts by lifecycle status.
    pub fn note_counts_impl(&self) -> crate::Result<AgentNoteCounts> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT status, COUNT(*) FROM agent_notes GROUP BY status")?;
        let rows = stmt
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row_usize(row, 1)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut counts = AgentNoteCounts::default();
        for (status, count) in rows {
            match status.parse::<AgentNoteStatus>()? {
                AgentNoteStatus::Active => counts.active = count,
                AgentNoteStatus::Unverified => counts.unverified = count,
                AgentNoteStatus::Stale => counts.stale = count,
                AgentNoteStatus::Superseded => counts.superseded = count,
                AgentNoteStatus::Forgotten => counts.forgotten = count,
                AgentNoteStatus::Invalid => counts.invalid = count,
            }
        }
        Ok(counts)
    }

    /// Query advisory notes with normal retrieval filtering pushed to SQL.
    pub fn query_notes_impl(&self, query: AgentNoteQuery) -> crate::Result<Vec<AgentNote>> {
        let conn = self.conn.lock();
        let mut sql = "SELECT note_id, target_kind, target_id, claim, evidence_json, created_by, created_at,
                              updated_at, confidence, status, source_hashes_json, graph_revision,
                              expires_on_drift, supersedes_json, superseded_by, verified_at, verified_by,
                              invalidated_by, source_store, advisory
                       FROM agent_notes WHERE 1=1"
            .to_string();
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(kind) = &query.target_kind {
            sql.push_str(" AND target_kind = ?");
            param_values.push(Box::new(kind.as_str().to_string()));
        }
        if let Some(target_id) = &query.target_id {
            sql.push_str(" AND target_id = ?");
            param_values.push(Box::new(target_id.clone()));
        }
        if !query.include_forgotten {
            sql.push_str(" AND status != 'forgotten'");
        }
        if !query.include_superseded {
            sql.push_str(" AND status != 'superseded'");
        }
        if !query.include_invalid {
            sql.push_str(" AND status != 'invalid'");
        }
        sql.push_str(" ORDER BY updated_at DESC, note_id ASC");

        let limit = query.limit.max(1);
        sql.push_str(" LIMIT ?");
        param_values.push(Box::new(limit as i64));

        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|p| p.as_ref()).collect();

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params_refs.as_slice(), note_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Return every stored note for audit and repair scanning.
    pub fn all_notes(&self) -> crate::Result<Vec<AgentNote>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT note_id, target_kind, target_id, claim, evidence_json, created_by, created_at,
                    updated_at, confidence, status, source_hashes_json, graph_revision,
                    expires_on_drift, supersedes_json, superseded_by, verified_at, verified_by,
                    invalidated_by, source_store, advisory
             FROM agent_notes",
        )?;
        let rows = stmt
            .query_map([], note_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}
