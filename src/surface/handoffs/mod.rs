//! Handoffs surface module.
//!
//! Aggregates repair recommendations, pending cross-link candidates, and git hotspots
//! into a prioritized list of actionable items.

mod hotspots;
mod overlay_candidates;
mod repair_log;
mod types;

pub use hotspots::read_hotspots;
pub use overlay_candidates::read_pending_candidates;
pub use repair_log::read_repair_log;
pub use types::{HandoffItem, HandoffPriority, HandoffSource, HandoffsRequest};

use std::path::Path;

/// Collect handoffs from all sources, combine and prioritize them.
pub fn collect_handoffs(
    repo_root: &Path,
    request: &HandoffsRequest,
) -> crate::Result<Vec<HandoffItem>> {
    let synrepo_dir = repo_root.join(".synrepo");
    let repair_log_path = synrepo_dir.join("state").join("repair-log.jsonl");
    let overlay_dir = synrepo_dir.join("overlay");

    // Read from all three sources
    let repair_items = read_repair_log(&repair_log_path, request.since_days).unwrap_or_default();
    let cross_link_items =
        read_pending_candidates(&overlay_dir, request.since_days).unwrap_or_default();
    let hotspot_items =
        read_hotspots(repo_root, request.since_days, request.limit).unwrap_or_default();

    // Combine all items
    let mut all_items: Vec<HandoffItem> = Vec::new();
    all_items.extend(repair_items);
    all_items.extend(cross_link_items);
    all_items.extend(hotspot_items);

    // Sort by priority (handles severity, confidence, recency, and surface type ordering)
    all_items.sort_by(|a, b| {
        // Primary: priority (already implements Ord for severity ordering)
        let priority_cmp = a.priority.cmp(&b.priority);
        if priority_cmp != std::cmp::Ordering::Equal {
            return priority_cmp;
        }

        // Secondary: source type (structural before overlay)
        let a_is_structural = matches!(a.item_type, HandoffSource::Repair | HandoffSource::Hotspot);
        let b_is_structural = matches!(b.item_type, HandoffSource::Repair | HandoffSource::Hotspot);
        match (a_is_structural, b_is_structural) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => std::cmp::Ordering::Equal,
        }
    });

    // Apply limit
    all_items.truncate(request.limit);

    Ok(all_items)
}

/// Format handoffs as markdown table.
pub fn to_markdown(items: &[HandoffItem]) -> String {
    if items.is_empty() {
        return "No handoff items found.".to_string();
    }

    let mut output = String::new();
    output.push_str("| Priority | Type | Source | Recommendation |\n");
    output.push_str("|----------|------|--------|----------------|\n");

    for item in items {
        let priority = item.priority.as_str();
        let item_type = match item.item_type {
            HandoffSource::Repair => "repair",
            HandoffSource::CrossLink => "cross_link",
            HandoffSource::Hotspot => "hotspot",
        };
        // Truncate long recommendations
        let rec = if item.recommendation.len() > 60 {
            format!("{}...", &item.recommendation[..57])
        } else {
            item.recommendation.clone()
        };
        output.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            priority, item_type, item.source, rec
        ));
    }

    output
}

/// Format handoffs as JSON.
pub fn to_json(items: &[HandoffItem]) -> String {
    serde_json::to_string_pretty(items).unwrap_or_else(|_| "[]".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_priority_ordering() {
        let items = vec![
            HandoffItem::new(
                "1".to_string(),
                HandoffSource::Hotspot,
                "a.rs".to_string(),
                "rec".to_string(),
                HandoffPriority::Low,
                "a.rs".to_string(),
                None,
            ),
            HandoffItem::new(
                "2".to_string(),
                HandoffSource::Repair,
                "b.rs".to_string(),
                "rec".to_string(),
                HandoffPriority::High,
                "b.rs".to_string(),
                None,
            ),
            HandoffItem::new(
                "3".to_string(),
                HandoffSource::CrossLink,
                "c.rs".to_string(),
                "rec".to_string(),
                HandoffPriority::Medium,
                "c.rs".to_string(),
                None,
            ),
            HandoffItem::new(
                "4".to_string(),
                HandoffSource::Repair,
                "d.rs".to_string(),
                "rec".to_string(),
                HandoffPriority::Critical,
                "d.rs".to_string(),
                None,
            ),
        ];

        // Test priority ordering via sort (ascending order)
        let mut sorted = items.clone();
        sorted.sort_by_key(|a| a.priority);

        assert_eq!(sorted[0].priority, HandoffPriority::Low);
        assert_eq!(sorted[1].priority, HandoffPriority::Medium);
        assert_eq!(sorted[2].priority, HandoffPriority::High);
        assert_eq!(sorted[3].priority, HandoffPriority::Critical);
    }

    #[test]
    fn test_to_markdown_empty() {
        let output = to_markdown(&[]);
        assert_eq!(output, "No handoff items found.");
    }

    #[test]
    fn test_to_markdown_formats_items() {
        let items = vec![HandoffItem::new(
            "1".to_string(),
            HandoffSource::Repair,
            "test.rs".to_string(),
            "Fix this".to_string(),
            HandoffPriority::High,
            "test.rs".to_string(),
            None,
        )];
        let output = to_markdown(&items);
        assert!(output.contains("| high | repair |"));
    }

    #[test]
    fn test_to_json_serializes() {
        let items = vec![HandoffItem::new(
            "1".to_string(),
            HandoffSource::Hotspot,
            "a.rs".to_string(),
            "rec".to_string(),
            HandoffPriority::Low,
            "a.rs".to_string(),
            None,
        )];
        let output = to_json(&items);
        assert!(output.contains("\"type\": \"hotspot\""));
    }
}
