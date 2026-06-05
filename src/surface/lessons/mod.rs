#![allow(missing_docs)]

//! Public saved-repo lesson helpers built on advisory overlay notes.

mod query;
#[cfg(test)]
mod tests;
mod ttl;
mod types;

use std::collections::HashSet;
use std::path::Path;

use time::{format_description::well_known::Rfc3339, Duration, OffsetDateTime};

use crate::config::Config;
use crate::overlay::{
    AgentNote, AgentNoteEvidence, AgentNoteQuery, AgentNoteSourceHash, AgentNoteStatus,
    AgentNoteTarget, AgentNoteTargetKind, OverlayStore,
};
use crate::store::{overlay::SqliteOverlayStore, sqlite::SqliteGraphStore};
use crate::structure::graph::GraphReader;

pub use ttl::{parse_cli_ttl, text_evidence, validate_ttl_seconds};
pub use types::{LessonAdd, LessonQuery, LessonView};

pub const LESSON_MARKER_KIND: &str = "synrepo.lesson";
pub const LESSON_MARKER_ID: &str = "v1";
pub const LESSON_EXPIRES_AT_KIND: &str = "synrepo.lesson.expires_at";
pub const DEFAULT_LESSON_LIMIT: usize = 10;
pub const MAX_LESSON_LIMIT: usize = 20;
pub const LESSON_SCAN_LIMIT: usize = 200;
pub const MAX_TTL_SECONDS: u64 = 365 * 24 * 60 * 60;
pub const MAX_EVIDENCE_TEXT_CHARS: usize = 500;

pub fn default_repo_target() -> AgentNoteTarget {
    AgentNoteTarget {
        kind: AgentNoteTargetKind::Repo,
        id: ".".to_string(),
    }
}

pub fn add_lesson(
    repo_root: Option<&Path>,
    overlay: &mut dyn OverlayStore,
    input: LessonAdd,
) -> crate::Result<LessonView> {
    let expires_at = input.ttl_seconds.map(ttl_expires_at).transpose()?.flatten();
    let mut note = AgentNote::new(
        AgentNoteTarget {
            kind: input.target_kind,
            id: input.target,
        },
        input.claim,
        input.created_by,
        input.confidence,
    );
    note.evidence = mark_lesson_evidence(input.evidence, expires_at)?;
    note.source_hashes = input.source_hashes;
    note.graph_revision = input.graph_revision;
    if let Some(root) = repo_root {
        fill_default_source_anchor(root, &mut note);
    }
    let inserted = overlay.insert_note(note)?;
    let stale = stale_ids(repo_root, std::slice::from_ref(&inserted));
    lesson_view(inserted, &stale)
}

pub fn search_lessons(
    repo_root: Option<&Path>,
    overlay: &dyn OverlayStore,
    request: LessonQuery,
) -> crate::Result<Vec<LessonView>> {
    let limit = query::normalize_limit(request.limit);
    let notes = overlay.query_notes(AgentNoteQuery {
        target_kind: request.target_kind,
        target_id: request.target,
        include_forgotten: request.include_hidden,
        include_superseded: request.include_hidden,
        include_invalid: request.include_hidden,
        limit: LESSON_SCAN_LIMIT,
    })?;
    let stale = stale_ids(repo_root, &notes);
    let now = OffsetDateTime::now_utc();
    let terms = request
        .query
        .as_deref()
        .map(query::query_terms)
        .unwrap_or_default();
    let mut ranked = Vec::new();
    for note in notes.into_iter().filter(is_lesson) {
        let expired = is_expired(&note, now);
        if !request.include_hidden && (expired || query::hidden_status(note.status)) {
            continue;
        }
        let score = if terms.is_empty() {
            1
        } else {
            query::lesson_score(&note, &terms, &stale, expired)
        };
        if score == 0 {
            continue;
        }
        ranked.push((score, note.updated_at, lesson_view(note, &stale)?));
    }
    ranked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    Ok(ranked
        .into_iter()
        .take(limit)
        .map(|(_, _, lesson)| lesson)
        .collect())
}

pub fn forget_lesson(
    repo_root: Option<&Path>,
    overlay: &mut dyn OverlayStore,
    lesson_id: &str,
    actor: &str,
    reason: Option<&str>,
) -> crate::Result<LessonView> {
    require_lesson(overlay, lesson_id)?;
    overlay.forget_note(lesson_id, actor, reason)?;
    let note = overlay
        .note_by_id(lesson_id)?
        .ok_or_else(|| lesson_not_found(lesson_id))?;
    let stale = stale_ids(repo_root, std::slice::from_ref(&note));
    lesson_view(note, &stale)
}

pub fn verify_lesson(
    repo_root: Option<&Path>,
    overlay: &mut dyn OverlayStore,
    lesson_id: &str,
    actor: &str,
    graph_revision: Option<u64>,
) -> crate::Result<LessonView> {
    require_lesson(overlay, lesson_id)?;
    let note = overlay.verify_note(lesson_id, actor, graph_revision)?;
    let stale = stale_ids(repo_root, std::slice::from_ref(&note));
    lesson_view(note, &stale)
}

pub fn is_lesson(note: &AgentNote) -> bool {
    note.evidence
        .iter()
        .any(|item| item.kind == LESSON_MARKER_KIND && item.id == LESSON_MARKER_ID)
}

pub fn public_evidence(note: &AgentNote) -> Vec<AgentNoteEvidence> {
    note.evidence
        .iter()
        .filter(|item| item.kind != LESSON_MARKER_KIND && item.kind != LESSON_EXPIRES_AT_KIND)
        .cloned()
        .collect()
}

pub fn expires_at(note: &AgentNote) -> Option<OffsetDateTime> {
    note.evidence
        .iter()
        .find(|item| item.kind == LESSON_EXPIRES_AT_KIND)
        .and_then(|item| OffsetDateTime::parse(&item.id, &Rfc3339).ok())
}

pub fn lesson_view(note: AgentNote, stale_ids: &HashSet<String>) -> crate::Result<LessonView> {
    let expires_at = expires_at(&note);
    let expired = expires_at.is_some_and(|ts| ts <= OffsetDateTime::now_utc());
    let freshness = if expired {
        "expired"
    } else if query::hidden_status(note.status) {
        "hidden"
    } else if note.status == AgentNoteStatus::Stale || stale_ids.contains(&note.note_id) {
        "stale"
    } else {
        "fresh"
    };
    let updated_at = note
        .updated_at
        .format(&Rfc3339)
        .map_err(|err| crate::Error::Other(anyhow::anyhow!(err)))?;
    let expires_at = expires_at
        .map(|ts| ts.format(&Rfc3339))
        .transpose()
        .map_err(|err| crate::Error::Other(anyhow::anyhow!(err)))?;
    let evidence = public_evidence(&note);
    Ok(LessonView {
        lesson_id: note.note_id,
        target_kind: note.target.kind.as_str().to_string(),
        target: note.target.id,
        claim: note.claim,
        status: note.status.as_str().to_string(),
        freshness: freshness.to_string(),
        expires_at,
        confidence: note.confidence.as_str().to_string(),
        updated_at,
        evidence,
        source_hashes: note.source_hashes,
        source_store: note.source_store,
        advisory: note.advisory,
    })
}

pub fn open_existing_lessons_overlay(repo_root: &Path) -> crate::Result<SqliteOverlayStore> {
    SqliteOverlayStore::open_existing(&Config::synrepo_dir(repo_root).join("overlay"))
}

fn require_lesson(overlay: &dyn OverlayStore, lesson_id: &str) -> crate::Result<()> {
    match overlay.note_by_id(lesson_id)? {
        Some(note) if is_lesson(&note) => Ok(()),
        _ => Err(lesson_not_found(lesson_id)),
    }
}

fn lesson_not_found(lesson_id: &str) -> crate::Error {
    crate::Error::Other(anyhow::anyhow!("lesson not found: {lesson_id}"))
}

fn mark_lesson_evidence(
    mut evidence: Vec<AgentNoteEvidence>,
    expires_at: Option<OffsetDateTime>,
) -> crate::Result<Vec<AgentNoteEvidence>> {
    evidence.retain(|item| item.kind != LESSON_MARKER_KIND && item.kind != LESSON_EXPIRES_AT_KIND);
    evidence.push(AgentNoteEvidence {
        kind: LESSON_MARKER_KIND.to_string(),
        id: LESSON_MARKER_ID.to_string(),
    });
    if let Some(expires_at) = expires_at {
        evidence.push(AgentNoteEvidence {
            kind: LESSON_EXPIRES_AT_KIND.to_string(),
            id: expires_at
                .format(&Rfc3339)
                .map_err(|err| crate::Error::Other(anyhow::anyhow!(err)))?,
        });
    }
    Ok(evidence)
}

fn ttl_expires_at(seconds: u64) -> crate::Result<Option<OffsetDateTime>> {
    ttl::validate_ttl_seconds(seconds)?;
    Ok(Some(
        OffsetDateTime::now_utc() + Duration::seconds(seconds as i64),
    ))
}

fn is_expired(note: &AgentNote, now: OffsetDateTime) -> bool {
    expires_at(note).is_some_and(|ts| ts <= now)
}

fn stale_ids(repo_root: Option<&Path>, notes: &[AgentNote]) -> HashSet<String> {
    let Some(repo_root) = repo_root else {
        return HashSet::new();
    };
    let graph_dir = Config::synrepo_dir(repo_root).join("graph");
    let Ok(graph) = SqliteGraphStore::open_existing(&graph_dir) else {
        return HashSet::new();
    };
    crate::store::overlay::current_drifted_note_ids(notes, &graph as &dyn GraphReader, None)
        .unwrap_or_default()
        .into_iter()
        .collect()
}

fn fill_default_source_anchor(repo_root: &Path, note: &mut AgentNote) {
    if !note.source_hashes.is_empty() || note.target.kind != AgentNoteTargetKind::Path {
        return;
    }
    let synrepo_dir = Config::synrepo_dir(repo_root);
    let Ok(graph) = SqliteGraphStore::open_existing(&synrepo_dir.join("graph")) else {
        return;
    };
    let Ok(Some(file)) = graph.file_by_path(&note.target.id) else {
        return;
    };
    note.source_hashes.push(AgentNoteSourceHash {
        path: file.path,
        hash: file.content_hash,
        root_id: Some(file.root_id),
    });
}
