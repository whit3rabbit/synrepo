//! SQLite row encoding for agent notes.

use rusqlite::{params, Connection, OptionalExtension};
use std::str::FromStr;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::overlay::{
    AgentNote, AgentNoteConfidence, AgentNoteStatus, AgentNoteTarget, AgentNoteTargetKind,
    AgentNoteTransition, AGENT_NOTE_SOURCE_STORE,
};

pub(super) fn format_time(ts: OffsetDateTime) -> crate::Result<String> {
    ts.format(&Rfc3339)
        .map_err(|err| crate::Error::Other(anyhow::anyhow!("invalid timestamp: {err}")))
}

fn parse_time(value: &str) -> crate::Result<OffsetDateTime> {
    OffsetDateTime::parse(value, &Rfc3339).map_err(|err| {
        crate::Error::Other(anyhow::anyhow!(
            "invalid stored agent-note timestamp `{value}`: {err}"
        ))
    })
}

fn parse_optional_time(value: Option<String>) -> crate::Result<Option<OffsetDateTime>> {
    value.as_deref().map(parse_time).transpose()
}

fn encode_json<T: serde::Serialize>(value: &T) -> crate::Result<String> {
    serde_json::to_string(value)
        .map_err(|err| crate::Error::Other(anyhow::anyhow!("failed to encode note JSON: {err}")))
}

fn decode_json<T: serde::de::DeserializeOwned>(value: String, column: &str) -> crate::Result<T> {
    serde_json::from_str(&value).map_err(|err| {
        crate::Error::Other(anyhow::anyhow!(
            "failed to decode agent-note `{column}` JSON: {err}"
        ))
    })
}

fn confidence_from_label(value: &str) -> crate::Result<AgentNoteConfidence> {
    AgentNoteConfidence::from_str(value)
}

pub(super) fn note_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentNote> {
    let note_id: String = row.get(0)?;
    let target_kind: String = row.get(1)?;
    let target_id: String = row.get(2)?;
    let claim: String = row.get(3)?;
    let evidence_json: String = row.get(4)?;
    let created_by: String = row.get(5)?;
    let created_at: String = row.get(6)?;
    let updated_at: String = row.get(7)?;
    let confidence: String = row.get(8)?;
    let status: String = row.get(9)?;
    let source_hashes_json: String = row.get(10)?;
    let graph_revision: Option<u64> = row.get(11)?;
    let expires_on_drift: i64 = row.get(12)?;
    let supersedes_json: String = row.get(13)?;
    let superseded_by: Option<String> = row.get(14)?;
    let verified_at: Option<String> = row.get(15)?;
    let verified_by: Option<String> = row.get(16)?;
    let invalidated_by: Option<String> = row.get(17)?;
    let source_store: String = row.get(18)?;
    let advisory: i64 = row.get(19)?;

    let decode = || -> crate::Result<AgentNote> {
        Ok(AgentNote {
            note_id,
            target: AgentNoteTarget {
                kind: AgentNoteTargetKind::from_str(&target_kind)?,
                id: target_id,
            },
            claim,
            evidence: decode_json(evidence_json, "evidence_json")?,
            created_by,
            created_at: parse_time(&created_at)?,
            updated_at: parse_time(&updated_at)?,
            confidence: confidence_from_label(&confidence)?,
            status: AgentNoteStatus::from_str(&status)?,
            source_hashes: decode_json(source_hashes_json, "source_hashes_json")?,
            graph_revision,
            expires_on_drift: expires_on_drift != 0,
            supersedes: decode_json(supersedes_json, "supersedes_json")?,
            superseded_by,
            verified_at: parse_optional_time(verified_at)?,
            verified_by,
            invalidated_by,
            source_store,
            advisory: advisory != 0,
        })
    };
    decode().map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(err))
    })
}

pub(super) fn insert_transition(conn: &Connection, tx: &AgentNoteTransition) -> crate::Result<()> {
    conn.execute(
        "INSERT INTO agent_note_transitions
            (note_id, action, previous_status, new_status, actor, reason, related_note, happened_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            tx.note_id,
            tx.action.as_str(),
            tx.previous_status.map(|status| status.as_str().to_string()),
            tx.new_status.as_str(),
            tx.actor,
            tx.reason,
            tx.related_note,
            format_time(tx.happened_at)?,
        ],
    )?;
    Ok(())
}

pub(super) fn upsert_note(conn: &Connection, note: &AgentNote) -> crate::Result<()> {
    conn.execute(
        "INSERT INTO agent_notes
            (note_id, target_kind, target_id, claim, evidence_json, created_by, created_at,
             updated_at, confidence, status, source_hashes_json, graph_revision, expires_on_drift,
             supersedes_json, superseded_by, verified_at, verified_by, invalidated_by,
             source_store, advisory)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)
         ON CONFLICT(note_id) DO UPDATE SET
             target_kind = excluded.target_kind,
             target_id = excluded.target_id,
             claim = excluded.claim,
             evidence_json = excluded.evidence_json,
             updated_at = excluded.updated_at,
             confidence = excluded.confidence,
             status = excluded.status,
             source_hashes_json = excluded.source_hashes_json,
             graph_revision = excluded.graph_revision,
             expires_on_drift = excluded.expires_on_drift,
             supersedes_json = excluded.supersedes_json,
             superseded_by = excluded.superseded_by,
             verified_at = excluded.verified_at,
             verified_by = excluded.verified_by,
             invalidated_by = excluded.invalidated_by,
             source_store = excluded.source_store,
             advisory = excluded.advisory",
        params![
            note.note_id,
            note.target.kind.as_str(),
            note.target.id,
            note.claim,
            encode_json(&note.evidence)?,
            note.created_by,
            format_time(note.created_at)?,
            format_time(note.updated_at)?,
            note.confidence.as_str(),
            note.status.as_str(),
            encode_json(&note.source_hashes)?,
            note.graph_revision,
            i64::from(note.expires_on_drift),
            encode_json(&note.supersedes)?,
            note.superseded_by,
            note.verified_at.map(format_time).transpose()?,
            note.verified_by,
            note.invalidated_by,
            AGENT_NOTE_SOURCE_STORE,
            1i64,
        ],
    )?;
    Ok(())
}

pub(super) fn note_by_id_conn(
    conn: &Connection,
    note_id: &str,
) -> crate::Result<Option<AgentNote>> {
    conn.query_row(
        "SELECT note_id, target_kind, target_id, claim, evidence_json, created_by, created_at,
                updated_at, confidence, status, source_hashes_json, graph_revision,
                expires_on_drift, supersedes_json, superseded_by, verified_at, verified_by,
                invalidated_by, source_store, advisory
         FROM agent_notes
         WHERE note_id = ?1",
        params![note_id],
        note_from_row,
    )
    .optional()
    .map_err(Into::into)
}
