use std::path::{Path, PathBuf};

use agent_config::{InstallStatus, Scope, ScopeKind};

use crate::agent_install::{SYNREPO_INSTALL_NAME, SYNREPO_INSTALL_OWNER};
use crate::registry::AgentEntry;

use super::{
    registry_scope, resolve_registry_path, ComponentKind, ComponentStatus, InstallScope,
    IntegrationComponent,
};

pub(super) fn resolve_mcp(
    repo_root: &Path,
    tool: &str,
    registry_agent: Option<&AgentEntry>,
    detected: bool,
) -> IntegrationComponent {
    if let Some(installer) = agent_config::mcp_by_id(tool) {
        if let Some(row) = row_from_agent_config(repo_root, installer.as_ref()) {
            return row;
        }
        if let Some(row) = row_from_registry(repo_root, registry_agent) {
            return row;
        }
        if let Some(path) = legacy_config_path(repo_root, tool) {
            return IntegrationComponent {
                kind: ComponentKind::Mcp,
                status: ComponentStatus::Installed,
                scope: InstallScope::Project,
                source: "legacy config".to_string(),
                path: Some(path),
            };
        }
        return missing_mcp(repo_root, installer.as_ref(), detected);
    }

    row_from_registry(repo_root, registry_agent).unwrap_or_else(|| IntegrationComponent {
        kind: ComponentKind::Mcp,
        status: ComponentStatus::Unsupported,
        scope: InstallScope::Unsupported,
        path: None,
        source: "unsupported".to_string(),
    })
}

fn row_from_agent_config(
    repo_root: &Path,
    installer: &dyn agent_config::McpSurface,
) -> Option<IntegrationComponent> {
    for (scope, label) in scopes(repo_root, installer.supported_mcp_scopes()) {
        let report = installer
            .mcp_status(&scope, SYNREPO_INSTALL_NAME, SYNREPO_INSTALL_OWNER)
            .ok()?;
        if installed(&report.status) {
            return Some(IntegrationComponent {
                kind: ComponentKind::Mcp,
                status: ComponentStatus::Installed,
                scope: label,
                path: report.config_path,
                source: source_label(&report.status).to_string(),
            });
        }
    }
    None
}

fn row_from_registry(
    repo_root: &Path,
    registry_agent: Option<&AgentEntry>,
) -> Option<IntegrationComponent> {
    let agent = registry_agent?;
    let path = agent
        .mcp_config_path
        .as_ref()
        .map(|path| resolve_registry_path(repo_root, path));
    let status = if path.as_ref().is_some_and(|path| path.exists()) {
        ComponentStatus::Installed
    } else if path.is_some() {
        ComponentStatus::Missing
    } else {
        ComponentStatus::Unsupported
    };
    let scope = if status == ComponentStatus::Unsupported {
        InstallScope::Unsupported
    } else {
        registry_scope(&agent.scope)
    };
    Some(IntegrationComponent {
        kind: ComponentKind::Mcp,
        status,
        scope,
        source: "registry record".to_string(),
        path,
    })
}

fn missing_mcp(
    repo_root: &Path,
    installer: &dyn agent_config::McpSurface,
    detected: bool,
) -> IntegrationComponent {
    let path = scopes(repo_root, installer.supported_mcp_scopes())
        .into_iter()
        .find_map(|(scope, _)| {
            installer
                .mcp_status(&scope, SYNREPO_INSTALL_NAME, SYNREPO_INSTALL_OWNER)
                .ok()
                .and_then(|report| report.config_path)
        });
    IntegrationComponent {
        kind: ComponentKind::Mcp,
        status: ComponentStatus::Missing,
        scope: InstallScope::Missing,
        path,
        source: detected_source(detected).to_string(),
    }
}

fn scopes(repo_root: &Path, supported: &[ScopeKind]) -> Vec<(Scope, InstallScope)> {
    let mut out = Vec::new();
    if supported.contains(&ScopeKind::Local) {
        out.push((Scope::Local(repo_root.to_path_buf()), InstallScope::Project));
    }
    if supported.contains(&ScopeKind::Global) {
        out.push((Scope::Global, InstallScope::Global));
    }
    out
}

fn installed(status: &InstallStatus) -> bool {
    matches!(
        status,
        InstallStatus::InstalledOwned { .. }
            | InstallStatus::PresentUnowned
            | InstallStatus::InstalledOtherOwner { .. }
    )
}

fn source_label(status: &InstallStatus) -> &'static str {
    match status {
        InstallStatus::InstalledOwned { .. } => "agent-config owned",
        InstallStatus::PresentUnowned => "legacy config",
        InstallStatus::InstalledOtherOwner { .. } => "agent-config other owner",
        _ => "not installed",
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

fn detected_source(detected: bool) -> &'static str {
    if detected {
        "target hint"
    } else {
        "not detected"
    }
}
