//! Freshness derivation for commentary entries.

use crate::overlay::{CommentaryEntry, CommentaryProvenance, FreshnessState};

/// Pass-id prefixes from older commentary generations whose entries should be
/// treated as stale regardless of content-hash match. When a new generation
/// ships (e.g. `commentary-v5`), add the previous generation's prefix here so
/// existing rows are forced through a refresh on the next sync.
const LEGACY_COMMENTARY_PASS_PREFIXES: &[&str] =
    &["commentary-v1", "commentary-v2", "commentary-v3"];

/// Whether `pass_id` was emitted by an older commentary generation that the
/// current code path cannot verify with content-hash equality.
///
/// Match is anchored at the version-number boundary (the prefix must be
/// followed by `-` or end-of-string) so `commentary-v10` never collides with
/// `commentary-v1`.
pub fn is_legacy_commentary_pass_id(pass_id: &str) -> bool {
    LEGACY_COMMENTARY_PASS_PREFIXES.iter().any(|prefix| {
        pass_id
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('-'))
    })
}

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
    if is_legacy_commentary_pass_id(&entry.provenance.pass_id) {
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
