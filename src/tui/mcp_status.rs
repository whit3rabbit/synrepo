//! Active-project MCP registration status for the dashboard MCP tab.

use std::path::{Path, PathBuf};

use agent_config::{InstallStatus, Scope};

use crate::agent_install::{SYNREPO_INSTALL_NAME, SYNREPO_INSTALL_OWNER};
use crate::bootstrap::runtime_probe::{
    all_agent_targets, detect_agent_targets, dirs_home, AgentTargetKind,
};
use crate::registry::{self, AgentEntry};
use crate::tui::probe::Severity;

#[cfg(test)]
mod tests;

/// One rendered MCP registration row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct McpStatusRow {
    /// Agent/tool display name.
    pub agent: String,
    /// Stable tool id such as `claude` or `codex`.
    pub tool: String,
    /// Registration status.
    pub status: McpStatus,
    /// Effective registration scope.
    pub scope: McpScope,
    /// Why synrepo believes this status applies.
    pub source: String,
    /// Config path that contains or should contain the MCP entry, if known.
    pub config_path: Option<PathBuf>,
}

impl McpStatusRow {
    /// Severity used by dashboard rendering.
    pub fn severity(&self) -> Severity {
        match self.status {
            McpStatus::Registered => Severity::Healthy,
            McpStatus::Missing | McpStatus::Unsupported => Severity::Stale,
        }
    }
}

/// MCP registration status.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum McpStatus {
    /// An MCP entry exists.
    Registered,
    /// No MCP entry was found for an MCP-capable agent.
    Missing,
    /// The tool does not have a known MCP-capable installer.
    Unsupported,
}

impl McpStatus {
    /// Stable display label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Registered => "registered",
            Self::Missing => "missing",
            Self::Unsupported => "unsupported",
        }
    }
}

/// Effective MCP scope label.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum McpScope {
    /// User/global agent configuration.
    Global,
    /// Project-local agent configuration.
    Project,
    /// No known MCP support.
    Unsupported,
    /// Supported, but no registration found.
    Missing,
}

impl McpScope {
    /// Stable display label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Project => "project",
            Self::Unsupported => "unsupported",
            Self::Missing => "missing",
        }
    }
}

/// Build active-project MCP rows without mutating any agent config.
pub fn build_mcp_status_rows(repo_root: &Path) -> Vec<McpStatusRow> {
    let registry_entry = registry::get(repo_root).ok().flatten();
    let detected = detect_agent_targets(repo_root, dirs_home().as_deref());
    let mut tools = default_tool_ids();
    if let Some(entry) = &registry_entry {
        for agent in &entry.agents {
            if !tools.iter().any(|tool| tool == &agent.tool) {
                tools.push(agent.tool.clone());
            }
        }
    }

    tools
        .into_iter()
        .map(|tool| {
            let registry_agent = registry_entry
                .as_ref()
                .and_then(|entry| entry.agents.iter().find(|agent| agent.tool == tool));
            let detected_target = target_for_id(&tool).filter(|target| detected.contains(target));
            resolve_tool_row(repo_root, &tool, registry_agent, detected_target)
        })
        .collect()
}

fn resolve_tool_row(
    repo_root: &Path,
    tool: &str,
    registry_agent: Option<&AgentEntry>,
    detected_target: Option<AgentTargetKind>,
) -> McpStatusRow {
    let agent = display_name(tool).to_string();
    if let Some(installer) = agent_config::mcp_by_id(tool) {
        if let Some(row) = row_from_agent_config(
            tool,
            &agent,
            installer.as_ref(),
            Scope::Local(repo_root.to_path_buf()),
            McpScope::Project,
        ) {
            return row;
        }
        if let Some(row) = row_from_agent_config(
            tool,
            &agent,
            installer.as_ref(),
            Scope::Global,
            McpScope::Global,
        ) {
            return row;
        }
        if let Some(row) = row_from_registry(repo_root, tool, &agent, registry_agent) {
            return row;
        }
        if let Some(path) = legacy_config_path(repo_root, tool) {
            return McpStatusRow {
                agent,
                tool: tool.to_string(),
                status: McpStatus::Registered,
                scope: McpScope::Project,
                source: "legacy config".to_string(),
                config_path: Some(path),
            };
        }
        return McpStatusRow {
            agent,
            tool: tool.to_string(),
            status: McpStatus::Missing,
            scope: McpScope::Missing,
            source: detected_source(detected_target),
            config_path: None,
        };
    }

    if let Some(row) = row_from_registry(repo_root, tool, &agent, registry_agent) {
        return row;
    }
    McpStatusRow {
        agent,
        tool: tool.to_string(),
        status: McpStatus::Unsupported,
        scope: McpScope::Unsupported,
        source: registry_agent
            .map(|_| "registry record".to_string())
            .unwrap_or_else(|| detected_source(detected_target)),
        config_path: None,
    }
}

fn row_from_agent_config(
    tool: &str,
    agent: &str,
    installer: &dyn agent_config::McpSurface,
    scope: Scope,
    mcp_scope: McpScope,
) -> Option<McpStatusRow> {
    let report = installer
        .mcp_status(&scope, SYNREPO_INSTALL_NAME, SYNREPO_INSTALL_OWNER)
        .ok()?;
    match report.status {
        InstallStatus::InstalledOwned { .. } => Some(McpStatusRow {
            agent: agent.to_string(),
            tool: tool.to_string(),
            status: McpStatus::Registered,
            scope: mcp_scope,
            source: "agent-config owned".to_string(),
            config_path: report.config_path,
        }),
        InstallStatus::PresentUnowned => Some(McpStatusRow {
            agent: agent.to_string(),
            tool: tool.to_string(),
            status: McpStatus::Registered,
            scope: mcp_scope,
            source: "legacy config".to_string(),
            config_path: report.config_path,
        }),
        _ => None,
    }
}

fn row_from_registry(
    repo_root: &Path,
    tool: &str,
    agent: &str,
    registry_agent: Option<&AgentEntry>,
) -> Option<McpStatusRow> {
    let registry_agent = registry_agent?;
    let path = registry_agent
        .mcp_config_path
        .as_ref()
        .map(|path| resolve_registry_path(repo_root, path));
    let (status, scope) = if path.is_some() {
        (McpStatus::Registered, registry_scope(&registry_agent.scope))
    } else {
        (McpStatus::Unsupported, McpScope::Unsupported)
    };
    Some(McpStatusRow {
        agent: agent.to_string(),
        tool: tool.to_string(),
        status,
        scope,
        source: "registry record".to_string(),
        config_path: path,
    })
}

fn registry_scope(scope: &str) -> McpScope {
    match scope {
        "global" => McpScope::Global,
        "project" => McpScope::Project,
        _ => McpScope::Project,
    }
}

fn resolve_registry_path(repo_root: &Path, path: &str) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path
    } else {
        repo_root.join(path)
    }
}

fn legacy_config_path(repo_root: &Path, tool: &str) -> Option<PathBuf> {
    match tool {
        "claude" => {
            let path = repo_root.join(".mcp.json");
            claude_mcp_registered(&path).then_some(path)
        }
        "codex" => {
            let path = repo_root.join(".codex").join("config.toml");
            codex_mcp_registered(&path).then_some(path)
        }
        _ => None,
    }
}

fn claude_mcp_registered(path: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return false;
    };
    value
        .get("mcpServers")
        .and_then(|servers| servers.get(SYNREPO_INSTALL_NAME))
        .is_some()
}

fn codex_mcp_registered(path: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };
    let Ok(doc) = text.parse::<toml_edit::DocumentMut>() else {
        return false;
    };
    doc.get("mcp_servers")
        .and_then(|item| item.as_table())
        .and_then(|table| table.get(SYNREPO_INSTALL_NAME))
        .is_some()
}

fn detected_source(target: Option<AgentTargetKind>) -> String {
    if target.is_some() {
        "target hint".to_string()
    } else {
        "not detected".to_string()
    }
}

fn default_tool_ids() -> Vec<String> {
    all_agent_targets()
        .iter()
        .map(|target| target.as_str().to_string())
        .collect()
}

fn target_for_id(id: &str) -> Option<AgentTargetKind> {
    all_agent_targets()
        .iter()
        .copied()
        .find(|target| target.as_str() == id)
}

fn display_name(id: &str) -> &str {
    match id {
        "claude" => "Claude Code",
        "cursor" => "Cursor",
        "codex" => "Codex CLI",
        "copilot" => "GitHub Copilot",
        "windsurf" => "Windsurf",
        "generic" => "Generic",
        other => other,
    }
}
