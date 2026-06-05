use std::collections::HashSet;

use crate::overlay::{AgentNote, AgentNoteStatus};

use super::{public_evidence, MAX_LESSON_LIMIT};

pub(super) fn normalize_limit(limit: usize) -> usize {
    limit.clamp(1, MAX_LESSON_LIMIT)
}

pub(super) fn hidden_status(status: AgentNoteStatus) -> bool {
    matches!(
        status,
        AgentNoteStatus::Forgotten | AgentNoteStatus::Superseded | AgentNoteStatus::Invalid
    )
}

pub(super) fn lesson_score(
    note: &AgentNote,
    terms: &[String],
    stale_ids: &HashSet<String>,
    expired: bool,
) -> i32 {
    let mut haystack = format!(
        "{} {} {}",
        note.claim.to_ascii_lowercase(),
        note.target.id.to_ascii_lowercase(),
        note.created_by.to_ascii_lowercase()
    );
    for evidence in public_evidence(note) {
        haystack.push(' ');
        haystack.push_str(&evidence.kind.to_ascii_lowercase());
        haystack.push(' ');
        haystack.push_str(&evidence.id.to_ascii_lowercase());
    }
    let mut score = 0;
    for term in terms {
        if haystack.contains(term) {
            score += 5;
        }
    }
    if score > 0 && !expired && !stale_ids.contains(&note.note_id) {
        score += 2;
    }
    if score > 0 && note.verified_at.is_some() {
        score += 1;
    }
    score
}

pub(super) fn query_terms(query: &str) -> Vec<String> {
    let mut terms = Vec::new();
    for raw in
        query.split(|ch: char| !ch.is_ascii_alphanumeric() && !matches!(ch, '_' | '-' | '.' | ':'))
    {
        let token = raw.trim().to_ascii_lowercase();
        if token.len() >= 2 && !terms.contains(&token) {
            terms.push(token);
        }
    }
    terms
}
