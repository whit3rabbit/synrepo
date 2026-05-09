use std::path::{Path, PathBuf};

use agent_config::{InstallStatus, Scope};

use crate::cli_support::agent_shims::{
    AgentTool, AutomationTier, SYNREPO_INSTALL_NAME, SYNREPO_INSTALL_OWNER,
};

use super::{McpRegistration, ShimFreshness};

pub(crate) fn classify_shim_freshness(
    repo_root: &Path,
    tool: AgentTool,
    scope: &Scope,
) -> ShimFreshness {
    let path = shim_path(repo_root, tool, scope);
    if !path.exists() {
        return ShimFreshness::Missing;
    }
    match std::fs::read_to_string(&path) {
        Ok(existing) if existing == tool.shim_content() => ShimFreshness::Current,
        Ok(existing) if existing.contains(tool.shim_spec_body()) => ShimFreshness::Current,
        Ok(_) | Err(_) => ShimFreshness::Stale,
    }
}

pub(crate) fn classify_mcp_registration(
    repo_root: &Path,
    tool: AgentTool,
    scope: &Scope,
) -> McpRegistration {
    if matches!(tool.automation_tier(), AutomationTier::ShimOnly) {
        return McpRegistration::Unsupported;
    }
    if mcp_registered(repo_root, tool, scope) {
        McpRegistration::Registered
    } else {
        McpRegistration::Missing
    }
}

pub(super) fn shim_path(repo_root: &Path, tool: AgentTool, scope: &Scope) -> PathBuf {
    tool.resolved_shim_output_path(scope)
        .unwrap_or_else(|| tool.output_path(repo_root))
}

pub(super) fn mcp_path(repo_root: &Path, tool: AgentTool, scope: &Scope) -> Option<PathBuf> {
    tool.resolved_mcp_config_path(scope).or_else(|| {
        tool.mcp_config_relative_path()
            .map(|rel| repo_root.join(rel))
    })
}

fn mcp_registered(repo_root: &Path, tool: AgentTool, scope: &Scope) -> bool {
    if let Some(installer) = tool.agent_config_id().and_then(agent_config::mcp_by_id) {
        return installer
            .mcp_status(scope, SYNREPO_INSTALL_NAME, SYNREPO_INSTALL_OWNER)
            .map(|report| {
                matches!(report.status, InstallStatus::InstalledOwned { ref owner } if owner == SYNREPO_INSTALL_OWNER)
            })
            .unwrap_or(false);
    }
    mcp_path(repo_root, tool, scope)
        .as_deref()
        .map(crate::cli_support::commands::mcp_config_has_synrepo)
        .transpose()
        .ok()
        .flatten()
        .unwrap_or(false)
}
