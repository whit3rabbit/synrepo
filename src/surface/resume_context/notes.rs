use std::path::Path;

use time::format_description::well_known::Rfc3339;

use crate::{
    overlay::{AgentNote, AgentNoteQuery, OverlayStore},
    store::overlay::SqliteOverlayStore,
};

use super::types::{AgentNoteSummary, SavedNotesSection};

pub(super) const NOTE_PREVIEW_CHARS: usize = 160;

pub(super) fn read_saved_notes(
    synrepo_dir: &Path,
    include_notes: bool,
    limit: usize,
) -> SavedNotesSection {
    if !include_notes {
        return SavedNotesSection {
            source_store: "overlay".to_string(),
            advisory: true,
            overlay_state: "disabled".to_string(),
            overlay_error: None,
            counts: None,
            count: 0,
            notes: Vec::new(),
        };
    }
    match SqliteOverlayStore::open_existing(&synrepo_dir.join("overlay")) {
        Ok(overlay) => {
            let counts = overlay.note_counts().ok();
            let notes = overlay
                .query_notes(AgentNoteQuery {
                    limit,
                    ..AgentNoteQuery::default()
                })
                .unwrap_or_default()
                .into_iter()
                .map(note_summary)
                .collect::<Vec<_>>();
            SavedNotesSection {
                source_store: "overlay".to_string(),
                advisory: true,
                overlay_state: "available".to_string(),
                overlay_error: None,
                counts,
                count: notes.len(),
                notes,
            }
        }
        Err(error) => SavedNotesSection {
            source_store: "overlay".to_string(),
            advisory: true,
            overlay_state: "unavailable".to_string(),
            overlay_error: Some(error.to_string()),
            counts: None,
            count: 0,
            notes: Vec::new(),
        },
    }
}

fn note_summary(note: AgentNote) -> AgentNoteSummary {
    AgentNoteSummary {
        note_id: note.note_id,
        target_kind: note.target.kind.as_str().to_string(),
        target: note.target.id,
        status: note.status.as_str().to_string(),
        confidence: note.confidence.as_str().to_string(),
        updated_at: note
            .updated_at
            .format(&Rfc3339)
            .unwrap_or_else(|_| String::new()),
        claim_preview: preview(&note.claim, NOTE_PREVIEW_CHARS),
        source_store: note.source_store,
        advisory: note.advisory,
    }
}

fn preview(text: &str, max_chars: usize) -> String {
    let mut iter = text.chars();
    let preview = iter.by_ref().take(max_chars).collect::<String>();
    if iter.next().is_some() {
        format!("{preview}...")
    } else {
        preview
    }
}
