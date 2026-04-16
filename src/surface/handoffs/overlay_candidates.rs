//! Overlay candidate reader for handoffs surface.
//!
//! Reads pending cross-link candidates from the overlay store.

use std::path::Path;

use crate::store::overlay::SqliteOverlayStore;

use super::types::{HandoffItem, HandoffPriority, HandoffSource};

/// Read pending cross-link candidates from the overlay store.
///
/// Returns an empty vec if the overlay is not materialized or has no pending candidates.
pub fn read_pending_candidates(
    overlay_dir: &Path,
    _since_days: u32,
) -> crate::Result<Vec<HandoffItem>> {
    // Try to open the overlay store (may not exist yet)
    let store = match SqliteOverlayStore::open_existing(overlay_dir) {
        Ok(s) => s,
        Err(_) => {
            // Overlay not materialized yet - return empty
            return Ok(Vec::new());
        }
    };

    // Use the public method to get active cross-links
    let rows = match store.active_cross_links() {
        Ok(r) => r,
        Err(_) => return Ok(Vec::new()),
    };

    // Convert to handoff items
    let items: Vec<HandoffItem> = rows
        .into_iter()
        .map(|(from_node, to_node, confidence_tier, rationale)| {
            let priority = match confidence_tier.as_str() {
                "high" => HandoffPriority::High,
                "medium" => HandoffPriority::Medium,
                _ => HandoffPriority::Low,
            };

            let source = format!("{} -> {}", from_node, to_node);
            let recommendation = if rationale.is_empty() {
                "Review cross-link candidate".to_string()
            } else {
                rationale
            };

            HandoffItem::new(
                format!("cross-link-{}", from_node),
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_read_pending_candidates_empty_overlay() {
        // When overlay doesn't exist, should return empty vec
        let temp_dir = TempDir::new().unwrap();
        let items = read_pending_candidates(temp_dir.path(), 30).unwrap();
        assert!(items.is_empty());
    }
}
