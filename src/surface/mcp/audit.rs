use schemars::JsonSchema;
use serde::Deserialize;

use crate::pipeline::recent_activity::{
    read_recent_activity, RecentActivityKind, RecentActivityQuery,
};

use super::findings;
use super::helpers::render_result;
use super::limits::{
    bounded_limit_value, DEFAULT_FINDINGS_LIMIT, DEFAULT_NOTES_LIMIT, MAX_FINDINGS_LIMIT,
    MAX_NOTES_LIMIT,
};
use super::SynrepoState;

/// Parameters for the `synrepo_findings` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindingsParams {
    pub repo_root: Option<std::path::PathBuf>,
    /// Optional node ID in display form.
    pub node_id: Option<String>,
    /// Optional kind to filter by (e.g. "references", "governs").
    pub kind: Option<String>,
    /// Optional freshness state to filter by.
    pub freshness: Option<String>,
    /// Maximum number of findings to return. Defaults to 25, capped at 50.
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_limit() -> u32 {
    DEFAULT_FINDINGS_LIMIT as u32
}

/// Parameters for the `synrepo_recent_activity` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct RecentActivityParams {
    pub repo_root: Option<std::path::PathBuf>,
    /// Activity kinds to include: "reconcile", "repair", "cross_link",
    /// "overlay_refresh", "hotspot". Defaults to all kinds.
    pub kinds: Option<Vec<String>>,
    /// Maximum entries to return (default 10, max 50).
    #[serde(default = "default_activity_limit")]
    pub limit: usize,
    /// Exclude entries older than this RFC 3339 timestamp.
    pub since: Option<String>,
}

fn default_activity_limit() -> usize {
    DEFAULT_NOTES_LIMIT
}

/// Parameters for the `synrepo_next_actions` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct NextActionsParams {
    pub repo_root: Option<std::path::PathBuf>,
    /// Maximum number of items to return. Defaults to 20.
    #[serde(default)]
    pub limit: Option<usize>,
    /// Only include items from the last N days. Defaults to 30.
    #[serde(default)]
    pub since_days: Option<u32>,
}

pub fn handle_findings(
    repo_root: &std::path::Path,
    node_id: Option<String>,
    kind: Option<String>,
    freshness: Option<String>,
    limit: u32,
) -> String {
    let capped =
        bounded_limit_value(limit as usize, DEFAULT_FINDINGS_LIMIT, MAX_FINDINGS_LIMIT) as u32;
    let result = findings::render_findings(repo_root, node_id, kind, freshness, capped);
    render_result(result)
}

pub fn handle_recent_activity(
    state: &SynrepoState,
    kinds: Option<Vec<String>>,
    limit: usize,
    since: Option<String>,
) -> String {
    let result = recent_activity_impl(state, kinds, limit, since);
    render_result(result)
}

fn recent_activity_impl(
    state: &SynrepoState,
    kinds: Option<Vec<String>>,
    limit: usize,
    since: Option<String>,
) -> anyhow::Result<serde_json::Value> {
    let parsed_kinds = kinds
        .map(|strs| {
            strs.into_iter()
                .map(|s| {
                    RecentActivityKind::parse_kind(&s)
                        .ok_or_else(|| anyhow::anyhow!("unknown activity kind: {s}"))
                })
                .collect::<anyhow::Result<Vec<_>>>()
        })
        .transpose()?;

    let synrepo_dir = crate::config::Config::synrepo_dir(&state.repo_root);
    let query = RecentActivityQuery {
        kinds: parsed_kinds,
        limit: bounded_limit_value(limit, DEFAULT_NOTES_LIMIT, MAX_NOTES_LIMIT),
        since,
    };
    let entries = read_recent_activity(&synrepo_dir, &state.repo_root, &state.config, query)?;
    Ok(serde_json::to_value(&entries)?)
}
