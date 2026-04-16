//! Repair-log reader for handoffs surface.
//!
//! Reads `.synrepo/state/repair-log.jsonl` and extracts actionable items.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use super::types::{HandoffItem, HandoffPriority, HandoffSource};

/// Read repair-log entries and convert to handoff items.
///
/// Filters for:
/// - Entries within the `since_days` window
/// - Items with actionable recommendations that are not "none"
pub fn read_repair_log<P: AsRef<Path>>(
    path: P,
    since_days: u32,
) -> std::io::Result<Vec<HandoffItem>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    // Calculate cutoff timestamp (seconds since epoch for `since_days` ago)
    let cutoff_timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64 - (since_days as i64 * 24 * 60 * 60))
        .unwrap_or(0);

    let mut items = Vec::new();
    let mut entry_idx = 0;

    for line in reader.lines() {
        let line = line?;
        entry_idx += 1;

        // Skip empty lines
        if line.trim().is_empty() {
            continue;
        }

        // Parse the JSON line
        let entry: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Get synced_at timestamp
        let synced_at = match entry.get("synced_at").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => continue,
        };

        // Parse the timestamp and filter by cutoff
        if let Some(entry_timestamp) = parse_rfc3339_to_timestamp(synced_at) {
            if entry_timestamp < cutoff_timestamp {
                continue;
            }
        }

        // Extract findings and look for actionable items
        let findings = match entry.get("findings_considered").and_then(|v| v.as_array()) {
            Some(arr) => arr,
            None => continue,
        };

        for finding in findings {
            let surface = finding
                .get("surface")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let severity = finding
                .get("severity")
                .and_then(|v| v.as_str())
                .unwrap_or("report_only");
            let recommended_action = finding
                .get("recommended_action")
                .and_then(|v| v.as_str())
                .unwrap_or("none");
            let notes = finding.get("notes").and_then(|v| v.as_str()).unwrap_or("");

            // Skip items that don't need action
            if recommended_action == "none" || recommended_action == "not_supported" {
                continue;
            }

            // Map severity to priority
            let priority = match severity {
                "critical" => HandoffPriority::Critical,
                "high" => HandoffPriority::High,
                "medium" => HandoffPriority::Medium,
                _ => HandoffPriority::Low,
            };

            // Build recommendation text
            let recommendation = if recommended_action.is_empty() || recommended_action == "none" {
                notes.to_string()
            } else {
                format!("{}: {}", recommended_action, notes)
            };

            if recommendation.is_empty() {
                continue;
            }

            let item = HandoffItem::new(
                format!("repair-{}-{}", entry_idx, surface),
                HandoffSource::Repair,
                surface.to_string(),
                recommendation,
                priority,
                ".synrepo/state/repair-log.jsonl".to_string(),
                None,
            );
            items.push(item);
        }
    }

    Ok(items)
}

/// Parse RFC3339 timestamp to seconds since epoch.
fn parse_rfc3339_to_timestamp(s: &str) -> Option<i64> {
    // Simple parsing: extract date-time components from RFC3339 format
    // Example: "2026-04-15T10:00:00Z"
    if s.len() >= 19 {
        let date_part = &s[..10]; // "2026-04-15"
        let _time_part = &s[11..19]; // "10:00:00"

        // Parse date components
        let parts: Vec<&str> = date_part.split('-').collect();
        if parts.len() != 3 {
            return None;
        }

        let year: i64 = parts[0].parse().ok()?;
        let month: u32 = parts[1].parse().ok()?;
        let day: u32 = parts[2].parse().ok()?;

        // Simple day count approximation (not perfect but good enough for filtering)
        // Using 365 days per year approximation
        let days_since_epoch = (year - 1970) * 365 + (month as i64 - 1) * 30 + day as i64;

        Some(days_since_epoch * 24 * 60 * 60)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_test_repair_log(entries: &[&str]) -> NamedTempFile {
        let mut file = tempfile::Builder::new()
            .prefix("repair-log")
            .suffix(".jsonl")
            .tempfile()
            .unwrap();
        for entry in entries {
            writeln!(file, "{}", entry).unwrap();
        }
        file.flush().unwrap();
        file
    }

    #[test]
    fn test_read_repair_log_basic() {
        let entry = r#"{"synced_at":"2026-04-15T10:00:00Z","findings_considered":[{"surface":"structural_refresh","severity":"high","recommended_action":"run_sync","notes":"Consider running synrepo sync to repair drift"}]}"#;

        let file = create_test_repair_log(&[entry]);
        let items = read_repair_log(file.path(), 30).unwrap();

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].item_type, HandoffSource::Repair);
        assert_eq!(items[0].priority, HandoffPriority::High);
    }

    #[test]
    fn test_read_repair_log_filters_none_actions() {
        let entry = r#"{"synced_at":"2026-04-15T10:00:00Z","findings_considered":[{"surface":"writer_lock","severity":"actionable","recommended_action":"none","notes":null}]}"#;

        let file = create_test_repair_log(&[entry]);
        let items = read_repair_log(file.path(), 30).unwrap();

        assert_eq!(items.len(), 0);
    }

    #[test]
    fn test_priority_mapping() {
        // Test that severity maps to correct priority
        let critical_entry = r#"{"synced_at":"2026-04-15T10:00:00Z","findings_considered":[{"surface":"test","severity":"critical","recommended_action":"fix","notes":"Critical issue"}]}"#;

        let file = create_test_repair_log(&[critical_entry]);
        let items = read_repair_log(file.path(), 30).unwrap();

        assert_eq!(items[0].priority, HandoffPriority::Critical);
    }
}
