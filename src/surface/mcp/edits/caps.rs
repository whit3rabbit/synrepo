use std::collections::BTreeSet;

use super::apply::AnchorEditRequest;

const HARD_MAX_EDITS: usize = 100;
const HARD_MAX_FILES: usize = 20;
const HARD_MAX_EDIT_BYTES: usize = 256 * 1024;
const HARD_MAX_TOTAL_TEXT_BYTES: usize = 512 * 1024;

pub(super) fn validate_edit_caps(edits: &[AnchorEditRequest]) -> anyhow::Result<()> {
    if edits.len() > HARD_MAX_EDITS {
        anyhow::bail!(
            "edits contains {} entries, exceeding hard limit {HARD_MAX_EDITS}",
            edits.len()
        );
    }

    let mut paths = BTreeSet::new();
    let mut total_text_bytes = 0usize;
    for edit in edits {
        paths.insert((
            edit.root_id.as_deref().unwrap_or("primary"),
            edit.path.as_str(),
        ));
        let edit_bytes = serde_json::to_vec(edit)?.len();
        if edit_bytes > HARD_MAX_EDIT_BYTES {
            anyhow::bail!(
                "single edit payload has {edit_bytes} bytes, exceeding hard limit {HARD_MAX_EDIT_BYTES}"
            );
        }
        total_text_bytes =
            total_text_bytes.saturating_add(edit.text.as_deref().map(str::len).unwrap_or(0));
    }

    if paths.len() > HARD_MAX_FILES {
        anyhow::bail!(
            "edits touch {} distinct files, exceeding hard limit {HARD_MAX_FILES}",
            paths.len()
        );
    }
    if total_text_bytes > HARD_MAX_TOTAL_TEXT_BYTES {
        anyhow::bail!(
            "submitted edit text has {total_text_bytes} bytes, exceeding hard limit {HARD_MAX_TOTAL_TEXT_BYTES}"
        );
    }

    Ok(())
}
