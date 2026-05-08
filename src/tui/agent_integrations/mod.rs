//! Read-only agent integration status shared by dashboard and status JSON.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::bootstrap::runtime_probe::{
    all_agent_targets, detect_agent_targets, dirs_home, AgentTargetKind,
};
use crate::registry::{self, AgentEntry};

mod context;
mod display;
mod hooks;
mod mcp;
#[cfg(test)]
mod tests;

pub use display::{
    build_agent_install_display_rows, summarize_agent_install_statuses, AgentInstallDisplayRow,
    AgentInstallSummary,
};

/// Full install-state row for one agent target.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AgentInstallStatus {
    /// Stable tool id such as `claude` or `codex`.
    pub tool: String,
    /// Human-readable agent name.
    pub display_name: String,
    /// Whether repo or home hints suggest the target is present.
    pub detected: bool,
    /// Skill or instruction install status.
    pub context: IntegrationComponent,
    /// MCP registration status.
    pub mcp: IntegrationComponent,
    /// Optional local hook status.
    pub hooks: HookInstallStatus,
    /// Roll-up health that ignores optional hooks.
    pub overall: AgentOverallStatus,
    /// Suggested next action, or `none`.
    pub next_action: String,
}

/// Status for context and MCP surfaces.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct IntegrationComponent {
    /// Surface kind.
    pub kind: ComponentKind,
    /// Install state.
    pub status: ComponentStatus,
    /// Effective scope.
    pub scope: InstallScope,
    /// File or config path, when known.
    pub path: Option<PathBuf>,
    /// Why this status was selected.
    pub source: String,
}

/// Optional hook install status.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct HookInstallStatus {
    /// Hook state.
    pub status: HookStatus,
    /// Hook config path for supported targets.
    pub path: Option<PathBuf>,
    /// Why this status was selected.
    pub source: String,
}

/// Surface kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentKind {
    /// Agent Skills `SKILL.md` context.
    Skill,
    /// Agent instruction or rule file context.
    Instructions,
    /// MCP server registration.
    Mcp,
    /// No supported surface.
    Unsupported,
}

impl ComponentKind {
    /// Stable display label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Skill => "skill",
            Self::Instructions => "instructions",
            Self::Mcp => "mcp",
            Self::Unsupported => "unsupported",
        }
    }
}

/// Context or MCP install state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentStatus {
    /// Surface is present.
    Installed,
    /// Surface is supported but absent.
    Missing,
    /// Surface is not supported for this target.
    Unsupported,
}

impl ComponentStatus {
    /// Stable display label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Installed => "installed",
            Self::Missing => "missing",
            Self::Unsupported => "unsupported",
        }
    }

    fn is_installed(self) -> bool {
        self == Self::Installed
    }

    fn is_unsupported(self) -> bool {
        self == Self::Unsupported
    }
}

/// Effective install scope.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallScope {
    /// Project-local agent config.
    Project,
    /// User/global agent config.
    Global,
    /// Supported, but no install found.
    Missing,
    /// No supported surface.
    Unsupported,
}

impl InstallScope {
    /// Stable display label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Global => "global",
            Self::Missing => "missing",
            Self::Unsupported => "unsupported",
        }
    }
}

/// Optional hook state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HookStatus {
    /// Local hooks are installed.
    Installed,
    /// Hooks are supported but absent. This is advisory only.
    Missing,
    /// The target has no synrepo hook installer.
    Unsupported,
}

impl HookStatus {
    /// Stable display label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Installed => "installed",
            Self::Missing => "missing_optional",
            Self::Unsupported => "unsupported",
        }
    }
}

/// Roll-up state for one target. Optional hooks do not affect this value.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentOverallStatus {
    /// Required supported surfaces are installed.
    Complete,
    /// One required supported surface is missing.
    Partial,
    /// Supported surfaces are absent.
    Missing,
    /// No context or MCP surface is supported for this target.
    Unsupported,
}

impl AgentOverallStatus {
    /// Stable display label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Partial => "partial",
            Self::Missing => "missing",
            Self::Unsupported => "unsupported",
        }
    }
}

impl AgentInstallStatus {
    fn new(
        repo_root: &Path,
        tool: String,
        registry_agent: Option<&AgentEntry>,
        detected: bool,
    ) -> Self {
        let display_name = display_name(&tool);
        let context = context::resolve_context(repo_root, &tool, registry_agent);
        let mcp = mcp::resolve_mcp(repo_root, &tool, registry_agent, detected);
        let hooks = hooks::resolve_hooks(repo_root, &tool);
        let overall = overall_status(&context, &mcp);
        let next_action = next_action(&tool, &context, &mcp, &hooks, overall);
        Self {
            tool,
            display_name,
            detected,
            context,
            mcp,
            hooks,
            overall,
            next_action,
        }
    }
}

/// Build integration rows without mutating any agent config.
pub fn build_agent_install_statuses(repo_root: &Path) -> Vec<AgentInstallStatus> {
    let registry_entry = registry::get(repo_root).ok().flatten();
    let detected_targets = detect_agent_targets(repo_root, dirs_home().as_deref());
    let detected_ids: BTreeSet<&str> = detected_targets
        .iter()
        .map(|target| target.as_str())
        .collect();
    integration_tool_ids(registry_entry.as_ref())
        .into_iter()
        .map(|tool| {
            let registry_agent = registry_entry
                .as_ref()
                .and_then(|entry| entry.agents.iter().find(|agent| agent.tool == tool));
            let detected = detected_ids.contains(tool.as_str());
            AgentInstallStatus::new(repo_root, tool, registry_agent, detected)
        })
        .collect()
}

fn overall_status(
    context: &IntegrationComponent,
    mcp: &IntegrationComponent,
) -> AgentOverallStatus {
    let context_installed = context.status.is_installed();
    let mcp_installed = mcp.status.is_installed();
    let context_unsupported = context.status.is_unsupported();
    let mcp_unsupported = mcp.status.is_unsupported();

    match (
        context_installed,
        mcp_installed,
        context_unsupported,
        mcp_unsupported,
    ) {
        (true, true, _, _) => AgentOverallStatus::Complete,
        (true, false, _, true) => AgentOverallStatus::Complete,
        (false, true, true, _) => AgentOverallStatus::Complete,
        (false, false, true, true) => AgentOverallStatus::Unsupported,
        (false, false, false, false) => AgentOverallStatus::Missing,
        _ => AgentOverallStatus::Partial,
    }
}

fn next_action(
    tool: &str,
    context: &IntegrationComponent,
    mcp: &IntegrationComponent,
    hooks: &HookInstallStatus,
    overall: AgentOverallStatus,
) -> String {
    if overall == AgentOverallStatus::Complete {
        if hooks.status == HookStatus::Missing {
            return format!("optional: synrepo setup {tool} --agent-hooks");
        }
        return "none".to_string();
    }
    if context.status == ComponentStatus::Missing && mcp.status == ComponentStatus::Missing {
        return format!("synrepo setup {tool} --project");
    }
    if context.status == ComponentStatus::Missing {
        return format!("synrepo agent-setup {tool}");
    }
    if mcp.status == ComponentStatus::Missing {
        return format!("synrepo setup {tool} --project");
    }
    if mcp.status == ComponentStatus::Unsupported && context.status == ComponentStatus::Missing {
        return format!("synrepo agent-setup {tool}");
    }
    "manual review".to_string()
}

fn integration_tool_ids(registry_entry: Option<&registry::ProjectEntry>) -> Vec<String> {
    let mut tools: Vec<String> = all_agent_targets()
        .iter()
        .map(|target| target.as_str().to_string())
        .collect();
    if let Some(entry) = registry_entry {
        for agent in &entry.agents {
            if !tools.iter().any(|tool| tool == &agent.tool) {
                tools.push(agent.tool.clone());
            }
        }
    }
    tools
}

fn display_name(id: &str) -> String {
    agent_config::by_id(id)
        .map(|agent| agent.display_name().to_string())
        .or_else(|| target_for_id(id).map(target_display_name))
        .unwrap_or_else(|| id.to_string())
}

fn target_for_id(id: &str) -> Option<AgentTargetKind> {
    all_agent_targets()
        .iter()
        .copied()
        .find(|target| target.as_str() == id)
}

fn target_display_name(target: AgentTargetKind) -> String {
    match target {
        AgentTargetKind::Amp => "Amp",
        AgentTargetKind::Antigravity => "Google Antigravity",
        AgentTargetKind::Claude => "Claude Code",
        AgentTargetKind::Cline => "Cline",
        AgentTargetKind::CodeBuddy => "CodeBuddy CLI",
        AgentTargetKind::Cursor => "Cursor",
        AgentTargetKind::Copilot => "GitHub Copilot",
        AgentTargetKind::Crush => "Charm Crush",
        AgentTargetKind::Codex => "Codex CLI",
        AgentTargetKind::Forge => "Forge",
        AgentTargetKind::Gemini => "Gemini CLI",
        AgentTargetKind::Hermes => "Hermes",
        AgentTargetKind::Iflow => "iFlow CLI",
        AgentTargetKind::Junie => "Junie",
        AgentTargetKind::Kilocode => "Kilo Code",
        AgentTargetKind::Opencode => "OpenCode",
        AgentTargetKind::Openclaw => "OpenClaw",
        AgentTargetKind::Pi => "Pi",
        AgentTargetKind::Qodercli => "Qoder CLI",
        AgentTargetKind::Qwen => "Qwen Code",
        AgentTargetKind::Roo => "Roo Code",
        AgentTargetKind::Tabnine => "Tabnine CLI",
        AgentTargetKind::Trae => "Trae",
        AgentTargetKind::Windsurf => "Windsurf",
    }
    .to_string()
}

pub(crate) fn registry_scope(scope: &str) -> InstallScope {
    match scope {
        "global" => InstallScope::Global,
        "project" => InstallScope::Project,
        _ => InstallScope::Project,
    }
}

pub(crate) fn resolve_registry_path(repo_root: &Path, path: &str) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path
    } else {
        repo_root.join(path)
    }
}
