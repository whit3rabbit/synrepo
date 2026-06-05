use std::path::Path;

use crate::config::Config;
use crate::pipeline::{
    git::{GitDegradedReason, GitIntelligenceContext, GitIntelligenceReadiness},
    git_intelligence::GitHistoryIndex,
};

use super::ActivityEntry;

/// Build git hotspot events from the in-memory `GitHistoryIndex`.
///
/// Returns a single entry with `state: "unavailable"` when git is absent.
/// Returns an empty list when git is available but no history was sampled.
pub fn read_hotspot_events(repo_root: &Path, config: &Config, limit: usize) -> Vec<ActivityEntry> {
    let context = GitIntelligenceContext::inspect(repo_root, config);

    // Bail early when the git repo itself is missing; don't try to walk history.
    if let GitIntelligenceReadiness::Degraded { ref reasons } = context.readiness() {
        if reasons.contains(&GitDegradedReason::RepositoryUnavailable) {
            return vec![ActivityEntry {
                kind: "hotspot".to_string(),
                timestamp: String::new(),
                payload: serde_json::json!({"state": "unavailable"}),
            }];
        }
    }

    let index = match GitHistoryIndex::build(&context, config.git_commit_depth as usize) {
        Ok(idx) => idx,
        Err(_) => {
            return vec![ActivityEntry {
                kind: "hotspot".to_string(),
                timestamp: String::new(),
                payload: serde_json::json!({"state": "unavailable"}),
            }];
        }
    };

    index
        .top_hotspots(limit)
        .into_iter()
        .map(|(path, touches)| ActivityEntry {
            kind: "hotspot".to_string(),
            timestamp: String::new(),
            payload: serde_json::json!({
                "path": path,
                "touches": touches,
                "source": "git_intelligence",
                "granularity": "file",
            }),
        })
        .collect()
}
