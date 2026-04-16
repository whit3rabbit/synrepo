//! Git hotspot reader for handoffs surface.
//!
//! Uses `GitHistoryIndex::top_hotspots` for lightweight hotspot extraction.

use std::path::Path;

use crate::config::Config;
use crate::pipeline::git::{GitDegradedReason, GitIntelligenceContext, GitIntelligenceReadiness};
use crate::pipeline::git_intelligence::GitHistoryIndex;

use super::types::{HandoffItem, HandoffPriority, HandoffSource};

/// Read git hotspots from the repository.
///
/// Returns files with high commit frequency in the given time window.
pub fn read_hotspots(
    repo_root: &Path,
    config: &Config,
    limit: usize,
) -> crate::Result<Vec<HandoffItem>> {
    let context = GitIntelligenceContext::inspect(repo_root, config);

    if let GitIntelligenceReadiness::Degraded { ref reasons } = context.readiness() {
        if reasons.contains(&GitDegradedReason::RepositoryUnavailable) {
            return Ok(Vec::new());
        }
    }

    let index = match GitHistoryIndex::build(&context, config.git_commit_depth as usize) {
        Ok(idx) => idx,
        Err(_) => return Ok(Vec::new()),
    };

    let items: Vec<HandoffItem> = index
        .top_hotspots(limit)
        .into_iter()
        .map(|(path, touches)| {
            let priority = if touches >= 20 {
                HandoffPriority::Critical
            } else if touches >= 10 {
                HandoffPriority::High
            } else if touches >= 5 {
                HandoffPriority::Medium
            } else {
                HandoffPriority::Low
            };

            let recommendation = format!("{} recent changes", touches);

            HandoffItem::new(
                format!("hotspot-{}", path.replace('/', "-")),
                HandoffSource::Hotspot,
                path.clone(),
                recommendation,
                priority,
                path,
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
        let temp_dir = TempDir::new().unwrap();
        let config = Config::default();
        let items = read_hotspots(temp_dir.path(), &config, 10).unwrap();
        assert!(items.is_empty());
    }
}
