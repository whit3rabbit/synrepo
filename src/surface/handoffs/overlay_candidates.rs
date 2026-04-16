//! Overlay candidate reader for handoffs surface.
//!
//! Reads pending cross-link candidates from the overlay store.

use std::path::Path;

use crate::overlay::ConfidenceTier;
use crate::store::overlay::SqliteOverlayStore;

use super::types::{HandoffItem, HandoffPriority, HandoffSource};

/// Read pending cross-link candidates from the overlay store.
///
/// Returns an empty vec if the overlay is not materialized or has no candidates.
pub fn read_pending_candidates(overlay_dir: &Path) -> crate::Result<Vec<HandoffItem>> {
    let store = match SqliteOverlayStore::open_existing(overlay_dir) {
        Ok(s) => s,
        Err(_) => return Ok(Vec::new()),
    };

    let candidates = match store.all_candidates(None) {
        Ok(c) => c,
        Err(_) => return Ok(Vec::new()),
    };

    let items: Vec<HandoffItem> = candidates
        .into_iter()
        .map(|link| {
            let priority = tier_to_priority(link.confidence_tier);
            let source = format!("{} -> {}", link.from, link.to);
            let recommendation = "Review cross-link candidate".to_string();

            HandoffItem::new(
                format!("cross-link-{}", link.from),
                HandoffSource::CrossLink,
                source,
                recommendation,
                priority,
                "overlay".to_string(),
                None,
            )
        })
        .collect();

    Ok(items)
}

fn tier_to_priority(tier: ConfidenceTier) -> HandoffPriority {
    match tier {
        ConfidenceTier::High => HandoffPriority::High,
        ConfidenceTier::ReviewQueue => HandoffPriority::Medium,
        ConfidenceTier::BelowThreshold => HandoffPriority::Low,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_read_pending_candidates_empty_overlay() {
        let temp_dir = TempDir::new().unwrap();
        let items = read_pending_candidates(temp_dir.path()).unwrap();
        assert!(items.is_empty());
    }
}
