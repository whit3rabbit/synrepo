//! Freshness derivation for commentary entries.

use crate::overlay::{CommentaryEntry, CommentaryProvenance, FreshnessState};

/// Derive the freshness state of a commentary entry relative to the current
/// content hash of the annotated node's file.
///
/// Returns `Invalid` if any required provenance field is empty (defensive;
/// `insert_commentary` rejects these on the way in). Returns `Fresh` on a hash
/// match, `Stale` on mismatch.
pub fn derive_freshness(entry: &CommentaryEntry, current_content_hash: &str) -> FreshnessState {
    if !has_complete_provenance(&entry.provenance) {
        return FreshnessState::Invalid;
    }
    if entry.provenance.pass_id.starts_with("commentary-v1")
        || entry.provenance.pass_id.starts_with("commentary-v2")
    {
        return FreshnessState::Stale;
    }
    if entry.provenance.source_content_hash == current_content_hash {
        FreshnessState::Fresh
    } else {
        FreshnessState::Stale
    }
}

pub(super) fn has_complete_provenance(prov: &CommentaryProvenance) -> bool {
    !prov.source_content_hash.is_empty()
        && !prov.pass_id.is_empty()
        && !prov.model_identity.is_empty()
}
