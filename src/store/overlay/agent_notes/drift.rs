use std::collections::BTreeSet;

use crate::overlay::{AgentNote, AgentNoteStatus};
use crate::structure::graph::GraphReader;

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
