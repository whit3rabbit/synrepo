//! Repair-log reader for handoffs surface.
//!
//! Reads `.synrepo/state/repair-log.jsonl` and extracts actionable items.

use std::path::Path;

use crate::pipeline::repair::{repair_log_path, RepairAction, ResolutionLogEntry, Severity};

use super::types::{HandoffItem, HandoffPriority, HandoffSource};

/// Read repair-log entries and convert to handoff items.
///
/// Filters for entries within the `since_days` window and findings with
/// actionable recommendations.
pub fn read_repair_log(synrepo_dir: &Path, since_days: u32) -> crate::Result<Vec<HandoffItem>> {
    let log_path = repair_log_path(synrepo_dir);
    if !log_path.exists() {
        return Ok(Vec::new());
    }

    let since_cutoff = since_cutoff_string(since_days);
    let content = std::fs::read_to_string(&log_path)?;

    let mut items = Vec::new();
    let mut entry_idx = 0;

    for line in content.lines().rev() {
        if line.trim().is_empty() {
            continue;
        }

        let entry: ResolutionLogEntry = match serde_json::from_str(line) {
            Ok(e) => e,
            Err(_) => continue,
        };
        entry_idx += 1;

        // Lexicographic comparison: RFC 3339 timestamps sort correctly as strings.
        if entry.synced_at.as_str() < since_cutoff.as_str() {
            continue;
        }

        for finding in &entry.findings_considered {
            if finding.recommended_action == RepairAction::None
                || finding.recommended_action == RepairAction::NotSupported
            {
                continue;
            }

            let priority = severity_to_priority(finding.severity);
            let recommendation = format!(
                "{}: {}",
                finding.recommended_action.as_str(),
                finding.notes.as_deref().unwrap_or("(no details)")
            );

            let item = HandoffItem::new(
                format!("repair-{}-{}", entry_idx, finding.surface.as_str()),
                HandoffSource::Repair,
                finding.surface.as_str().to_string(),
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

/// Map `Severity` to `HandoffPriority`.
///
/// `Severity` has four variants: `Actionable`, `ReportOnly`, `Blocked`,
/// `Unsupported`. The handoffs surface treats actionable findings as high
/// priority (they have auto-repair available), and everything else as medium
/// or lower.
fn severity_to_priority(severity: Severity) -> HandoffPriority {
    match severity {
        Severity::Actionable => HandoffPriority::High,
        Severity::Blocked => HandoffPriority::Medium,
        Severity::ReportOnly => HandoffPriority::Low,
        Severity::Unsupported => HandoffPriority::Low,
    }
}

/// Compute an RFC 3339 cutoff string for `since_days` days ago.
///
/// Uses the `time` crate for correct calendar arithmetic.
fn since_cutoff_string(since_days: u32) -> String {
    let cutoff = time::OffsetDateTime::now_utc() - time::Duration::days(since_days as i64);
    cutoff
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_severity_to_priority_mapping() {
        assert_eq!(
            severity_to_priority(Severity::Actionable),
            HandoffPriority::High
        );
        assert_eq!(
            severity_to_priority(Severity::Blocked),
            HandoffPriority::Medium
        );
        assert_eq!(
            severity_to_priority(Severity::ReportOnly),
            HandoffPriority::Low
        );
        assert_eq!(
            severity_to_priority(Severity::Unsupported),
            HandoffPriority::Low
        );
    }

    #[test]
    fn test_read_repair_log_missing_file() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let items = read_repair_log(temp_dir.path(), 30).unwrap();
        assert!(items.is_empty());
    }
}
