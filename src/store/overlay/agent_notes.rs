//! Agent-note CRUD, lifecycle transitions, and drift helpers.

mod codec;

use codec::{format_time, insert_transition, note_by_id_conn, note_from_row, upsert_note};
use rusqlite::params;
use std::collections::BTreeSet;
use time::OffsetDateTime;

use crate::overlay::{
    AgentNote, AgentNoteCounts, AgentNoteQuery, AgentNoteStatus, AgentNoteTransition,
    AgentNoteTransitionAction,
};
use crate::structure::graph::GraphReader;

use super::{sqlite_values::row_usize, with_write_transaction, SqliteOverlayStore};

impl SqliteOverlayStore {
    /// Insert an advisory agent note and append its creation transition.
    pub fn insert_note_impl(&mut self, mut note: AgentNote) -> crate::Result<AgentNote> {
        note.normalize_for_insert()?;
        note.updated_at = OffsetDateTime::now_utc();
        let conn = self.conn.lock();
        with_write_transaction(&conn, |conn| {
            upsert_note(conn, &note)?;
            insert_transition(
                conn,
                &AgentNoteTransition {
                    note_id: note.note_id.clone(),
                    action: AgentNoteTransitionAction::Add,
                    previous_status: None,
                    new_status: note.status,
                    actor: note.created_by.clone(),
                    reason: None,
                    related_note: None,
                    happened_at: note.updated_at,
                },
            )
        })?;
        Ok(note)
    }

    /// Query advisory notes.
    pub fn query_notes_impl_trait(&self, query: AgentNoteQuery) -> crate::Result<Vec<AgentNote>> {
        self.query_notes_impl(query)
    }

    /// Return one note by ID.
    pub fn note_by_id_impl(&self, note_id: &str) -> crate::Result<Option<AgentNote>> {
        let conn = self.conn.lock();
        note_by_id_conn(&conn, note_id)
    }

    /// Link two notes without changing either claim.
    pub fn link_note_impl(
        &mut self,
        from_note: &str,
        to_note: &str,
        actor: &str,
    ) -> crate::Result<()> {
        let conn = self.conn.lock();
        let Some(from) = note_by_id_conn(&conn, from_note)? else {
            return Err(crate::Error::Other(anyhow::anyhow!(
                "note not found: {from_note}"
            )));
        };
        if note_by_id_conn(&conn, to_note)?.is_none() {
            return Err(crate::Error::Other(anyhow::anyhow!(
                "note not found: {to_note}"
            )));
        }
        let now = OffsetDateTime::now_utc();
        with_write_transaction(&conn, |conn| {
            conn.execute(
                "INSERT OR IGNORE INTO agent_note_links (from_note, to_note, actor, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![from_note, to_note, actor, format_time(now)?],
            )?;
            insert_transition(
                conn,
                &AgentNoteTransition {
                    note_id: from.note_id,
                    action: AgentNoteTransitionAction::Link,
                    previous_status: Some(from.status),
                    new_status: from.status,
                    actor: actor.to_string(),
                    reason: None,
                    related_note: Some(to_note.to_string()),
                    happened_at: now,
                },
            )
        })
    }

    /// Supersede an existing note with a replacement note.
    pub fn supersede_note_impl(
        &mut self,
        old_note: &str,
        mut replacement: AgentNote,
        actor: &str,
    ) -> crate::Result<AgentNote> {
        replacement.normalize_for_insert()?;
        let conn = self.conn.lock();
        let Some(mut old) = note_by_id_conn(&conn, old_note)? else {
            return Err(crate::Error::Other(anyhow::anyhow!(
                "note not found: {old_note}"
            )));
        };
        let previous = old.status;
        let now = OffsetDateTime::now_utc();
        old.superseded_by = Some(replacement.note_id.clone());
        old.status = AgentNoteStatus::Superseded;
        old.updated_at = now;
        replacement.supersedes.push(old.note_id.clone());
        replacement.updated_at = now;
        if replacement.evidence.is_empty() && replacement.status == AgentNoteStatus::Active {
            replacement.status = AgentNoteStatus::Unverified;
        }
        with_write_transaction(&conn, |conn| {
            upsert_note(conn, &old)?;
            upsert_note(conn, &replacement)?;
            insert_transition(
                conn,
                &AgentNoteTransition {
                    note_id: old.note_id,
                    action: AgentNoteTransitionAction::Supersede,
                    previous_status: Some(previous),
                    new_status: AgentNoteStatus::Superseded,
                    actor: actor.to_string(),
                    reason: None,
                    related_note: Some(replacement.note_id.clone()),
                    happened_at: now,
                },
            )?;
            insert_transition(
                conn,
                &AgentNoteTransition {
                    note_id: replacement.note_id.clone(),
                    action: AgentNoteTransitionAction::Add,
                    previous_status: None,
                    new_status: replacement.status,
                    actor: actor.to_string(),
                    reason: Some("superseding replacement".to_string()),
                    related_note: Some(old_note.to_string()),
                    happened_at: now,
                },
            )
        })?;
        Ok(replacement)
    }

    /// Hide a note from normal retrieval while retaining audit history.
    pub fn forget_note_impl(
        &mut self,
        note_id: &str,
        actor: &str,
        reason: Option<&str>,
    ) -> crate::Result<()> {
        let conn = self.conn.lock();
        let Some(mut note) = note_by_id_conn(&conn, note_id)? else {
            return Err(crate::Error::Other(anyhow::anyhow!(
                "note not found: {note_id}"
            )));
        };
        let previous = note.status;
        let now = OffsetDateTime::now_utc();
        note.status = AgentNoteStatus::Forgotten;
        note.updated_at = now;
        with_write_transaction(&conn, |conn| {
            upsert_note(conn, &note)?;
            insert_transition(
                conn,
                &AgentNoteTransition {
                    note_id: note_id.to_string(),
                    action: AgentNoteTransitionAction::Forget,
                    previous_status: Some(previous),
                    new_status: AgentNoteStatus::Forgotten,
                    actor: actor.to_string(),
                    reason: reason.map(ToOwned::to_owned),
                    related_note: None,
                    happened_at: now,
                },
            )
        })
    }

    /// Verify a note against current source-derived facts.
    pub fn verify_note_impl(
        &mut self,
        note_id: &str,
        actor: &str,
        graph_revision: Option<u64>,
    ) -> crate::Result<AgentNote> {
        let conn = self.conn.lock();
        let Some(mut note) = note_by_id_conn(&conn, note_id)? else {
            return Err(crate::Error::Other(anyhow::anyhow!(
                "note not found: {note_id}"
            )));
        };
        let previous = note.status;
        let now = OffsetDateTime::now_utc();
        note.status = AgentNoteStatus::Active;
        note.updated_at = now;
        note.verified_at = Some(now);
        note.verified_by = Some(actor.to_string());
        if let Some(rev) = graph_revision {
            note.graph_revision = Some(rev);
        }
        with_write_transaction(&conn, |conn| {
            upsert_note(conn, &note)?;
            insert_transition(
                conn,
                &AgentNoteTransition {
                    note_id: note_id.to_string(),
                    action: AgentNoteTransitionAction::Verify,
                    previous_status: Some(previous),
                    new_status: AgentNoteStatus::Active,
                    actor: actor.to_string(),
                    reason: None,
                    related_note: None,
                    happened_at: now,
                },
            )
        })?;
        Ok(note)
    }

    /// Mark notes stale when their drift anchors no longer match.
    pub fn mark_stale_notes_impl(
        &mut self,
        stale_note_ids: &[String],
        actor: &str,
    ) -> crate::Result<usize> {
        let conn = self.conn.lock();
        let now = OffsetDateTime::now_utc();
        let mut changed = 0usize;
        with_write_transaction(&conn, |conn| {
            for note_id in stale_note_ids {
                let Some(mut note) = note_by_id_conn(conn, note_id)? else {
                    continue;
                };
                if matches!(
                    note.status,
                    AgentNoteStatus::Forgotten
                        | AgentNoteStatus::Superseded
                        | AgentNoteStatus::Invalid
                        | AgentNoteStatus::Stale
                ) {
                    continue;
                }
                let previous = note.status;
                note.status = AgentNoteStatus::Stale;
                note.updated_at = now;
                note.invalidated_by = Some(actor.to_string());
                upsert_note(conn, &note)?;
                insert_transition(
                    conn,
                    &AgentNoteTransition {
                        note_id: note_id.clone(),
                        action: AgentNoteTransitionAction::Invalidate,
                        previous_status: Some(previous),
                        new_status: AgentNoteStatus::Stale,
                        actor: actor.to_string(),
                        reason: Some("drift anchor mismatch".to_string()),
                        related_note: None,
                        happened_at: now,
                    },
                )?;
                changed += 1;
            }
            Ok(())
        })?;
        Ok(changed)
    }

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
}

impl SqliteOverlayStore {
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

/// Return note IDs whose anchors no longer match current graph state.
pub fn current_drifted_note_ids(
    notes: &[AgentNote],
    graph: &dyn GraphReader,
    current_graph_revision: Option<u64>,
) -> crate::Result<Vec<String>> {
    let mut stale = BTreeSet::new();
    for note in notes {
        if !note.expires_on_drift {
            continue;
        }
        if !matches!(
            note.status,
            AgentNoteStatus::Active | AgentNoteStatus::Unverified
        ) {
            continue;
        }
        if let (Some(stored), Some(current)) = (note.graph_revision, current_graph_revision) {
            if stored != current {
                stale.insert(note.note_id.clone());
                continue;
            }
        }
        for anchor in &note.source_hashes {
            let file = match &anchor.root_id {
                Some(root_id) => graph.file_by_root_path(root_id, &anchor.path)?,
                None => graph.file_by_path(&anchor.path)?,
            };
            match file {
                Some(f) if f.content_hash == anchor.hash => {}
                _ => {
                    stale.insert(note.note_id.clone());
                    break;
                }
            }
        }
        for evidence in &note.evidence {
            let missing = match evidence.kind.as_str() {
                "file" => evidence
                    .id
                    .parse()
                    .ok()
                    .and_then(|id| graph.get_file(id).ok().flatten())
                    .is_none(),
                "symbol" => evidence
                    .id
                    .parse()
                    .ok()
                    .and_then(|id| graph.get_symbol(id).ok().flatten())
                    .is_none(),
                "concept" => evidence
                    .id
                    .parse()
                    .ok()
                    .and_then(|id| graph.get_concept(id).ok().flatten())
                    .is_none(),
                _ => false,
            };
            if missing {
                stale.insert(note.note_id.clone());
                break;
            }
        }
    }
    Ok(stale.into_iter().collect())
}
