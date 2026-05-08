//! Agent-note CRUD, lifecycle transitions, and drift helpers.

mod codec;
mod drift;
mod read;

use codec::{format_time, insert_transition, note_by_id_conn, upsert_note};
use rusqlite::params;
use time::OffsetDateTime;

use crate::overlay::{
    AgentNote, AgentNoteQuery, AgentNoteStatus, AgentNoteTransition, AgentNoteTransitionAction,
};

pub use drift::current_drifted_note_ids;

use super::{with_write_transaction, SqliteOverlayStore};

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
}
