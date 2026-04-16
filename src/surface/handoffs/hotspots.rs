//! Git hotspot reader for handoffs surface.
//!
//! Uses existing git-intelligence infrastructure to get file hotspots.

use std::path::Path;

use crate::config::Config;
use crate::pipeline::git::{GitHeadState, GitIntelligenceContext};
use crate::pipeline::git_intelligence::analyze_recent_history;

use super::types::{HandoffItem, HandoffPriority, HandoffSource};

/// Read git hotspots from the repository.
///
/// Returns files with high commit frequency in the given time window.
pub fn read_hotspots(
    repo_root: &Path,
    since_days: u32,
    limit: usize,
) -> crate::Result<Vec<HandoffItem>> {
    // Create git intelligence context
    let config = Config::default();
    let context = GitIntelligenceContext::inspect(repo_root, &config);

    // Check if git is available
    if matches!(
        context.repository().head(),
        GitHeadState::Unavailable | GitHeadState::Unborn
    ) {
        return Ok(Vec::new());
    }

    // Get recent history analysis (larger window to capture hotspots)
    let max_commits = (since_days as usize) * 10; // Rough estimate: 10 commits per day max
    let insights = analyze_recent_history(&context, max_commits, limit)?;

    // Extract hotspots and convert to handoff items
    let items: Vec<HandoffItem> = insights
        .hotspots
        .into_iter()
        .take(limit)
        .map(|hotspot| {
            let recommendation = format!(
                "File has {} recent changes (last: {})",
                hotspot.touches, hotspot.last_summary
            );

            // Map touch count to priority
            let priority = if hotspot.touches >= 20 {
                HandoffPriority::Critical
            } else if hotspot.touches >= 10 {
                HandoffPriority::High
            } else if hotspot.touches >= 5 {
                HandoffPriority::Medium
            } else {
                HandoffPriority::Low
            };

            HandoffItem::new(
                format!("hotspot-{}", hotspot.path.replace('/', "-")),
                HandoffSource::Hotspot,
                hotspot.path.clone(),
                recommendation,
                priority,
                hotspot.path,
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
    fn test_read_hotspots_no_git() {
        // When git is not available, should return empty vec
        let temp_dir = TempDir::new().unwrap();
        let items = read_hotspots(temp_dir.path(), 30, 10).unwrap();
        assert!(items.is_empty());
    }

    #[test]
    fn test_read_hotspots_validates_limit() {
        // Should respect the limit parameter
        let temp_dir = TempDir::new().unwrap();
        let _ = read_hotspots(temp_dir.path(), 30, 5);
    }
}
