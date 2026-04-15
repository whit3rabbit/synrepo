use schemars::JsonSchema;
use serde::Deserialize;

use crate::pipeline::recent_activity::{
    read_recent_activity, RecentActivityKind, RecentActivityQuery,
};

use super::findings;
use super::helpers::render_result;
use super::SynrepoState;

/// Parameters for the `synrepo_findings` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindingsParams {
    /// Optional node ID in display form.
    pub node_id: Option<String>,
    /// Optional kind to filter by (e.g. "references", "governs").
    pub kind: Option<String>,
    /// Optional freshness state to filter by.
    pub freshness: Option<String>,
    /// Maximum number of findings to return. Defaults to 20.
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_limit() -> u32 {
    20
}

/// Parameters for the `synrepo_recent_activity` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct RecentActivityParams {
    /// Activity kinds to include: "reconcile", "repair", "cross_link",
    /// "overlay_refresh", "hotspot". Defaults to all kinds.
    pub kinds: Option<Vec<String>>,
    /// Maximum entries to return (default 20, max 200).
    #[serde(default = "default_activity_limit")]
    pub limit: usize,
    /// Exclude entries older than this RFC 3339 timestamp.
    pub since: Option<String>,
}

fn default_activity_limit() -> usize {
    20
}

pub fn handle_findings(
    repo_root: &std::path::Path,
    node_id: Option<String>,
    kind: Option<String>,
    freshness: Option<String>,
    limit: u32,
) -> String {
    let result = findings::render_findings(repo_root, node_id, kind, freshness, limit);
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
        limit,
        since,
    };
    let entries = read_recent_activity(&synrepo_dir, &state.repo_root, &state.config, query)?;
    Ok(serde_json::to_value(&entries)?)
}
