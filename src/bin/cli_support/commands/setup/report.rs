use std::path::{Path, PathBuf};

use crate::cli_support::agent_shims::{AgentTool, AutomationTier};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ClientOutcome {
    Detected,
    Written,
    Registered,
    Current,
    Skipped,
    Unsupported,
    Stale,
    Failed,
}

impl ClientOutcome {
    fn as_str(self) -> &'static str {
        match self {
            Self::Detected => "detected",
            Self::Written => "written",
            Self::Registered => "registered",
            Self::Current => "current",
            Self::Skipped => "skipped",
            Self::Unsupported => "unsupported",
            Self::Stale => "stale",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SetupScope {
    Project,
    #[allow(dead_code)]
    Global,
}

impl SetupScope {
    fn as_str(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Global => "global",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SetupPath {
    pub scope: SetupScope,
    pub path: PathBuf,
}

impl SetupPath {
    fn project(path: PathBuf) -> Self {
        Self {
            scope: SetupScope::Project,
            path,
        }
    }

    fn render(&self, repo_root: &Path) -> String {
        let display = self
            .path
            .strip_prefix(repo_root)
            .map(|rel| rel.display().to_string())
            .unwrap_or_else(|_| self.path.display().to_string());
        format!("{} {}", self.scope.as_str(), display)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShimFreshness {
    Missing,
    Current,
    Stale,
}

impl ShimFreshness {
    fn as_str(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Current => "current",
            Self::Stale => "stale",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum McpRegistration {
    Registered,
    Missing,
    Unsupported,
}

impl McpRegistration {
    fn as_str(self) -> &'static str {
        match self {
            Self::Registered => "registered",
            Self::Missing => "missing",
            Self::Unsupported => "unsupported",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ClientSetupEntry {
    pub tool: AgentTool,
    pub outcomes: Vec<ClientOutcome>,
    pub shim: ShimFreshness,
    pub shim_path: SetupPath,
    pub mcp: McpRegistration,
    pub mcp_path: Option<SetupPath>,
    pub error: Option<String>,
}

impl ClientSetupEntry {
    pub(crate) fn skipped(repo_root: &Path, tool: AgentTool, detected: bool) -> Self {
        let mut outcomes = Vec::new();
        push_if(&mut outcomes, detected, ClientOutcome::Detected);
        outcomes.push(ClientOutcome::Skipped);
        Self {
            tool,
            outcomes,
            shim: classify_shim_freshness(repo_root, tool),
            shim_path: SetupPath::project(tool.output_path(repo_root)),
            mcp: classify_mcp_registration(repo_root, tool),
            mcp_path: tool
                .mcp_config_relative_path()
                .map(|rel| SetupPath::project(repo_root.join(rel))),
            error: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ClientBefore {
    shim: ShimFreshness,
}

impl ClientBefore {
    pub(crate) fn observe(repo_root: &Path, tool: AgentTool) -> Self {
        Self {
            shim: classify_shim_freshness(repo_root, tool),
        }
    }
}

pub(crate) fn entry_after_success(
    repo_root: &Path,
    tool: AgentTool,
    before: ClientBefore,
    detected: bool,
) -> ClientSetupEntry {
    let shim = classify_shim_freshness(repo_root, tool);
    let mcp = classify_mcp_registration(repo_root, tool);
    let mut outcomes = Vec::new();
    push_if(&mut outcomes, detected, ClientOutcome::Detected);
    match shim {
        ShimFreshness::Current => {
            if matches!(before.shim, ShimFreshness::Missing | ShimFreshness::Stale) {
                outcomes.push(ClientOutcome::Written);
            } else {
                outcomes.push(ClientOutcome::Current);
            }
        }
        ShimFreshness::Stale => outcomes.push(ClientOutcome::Stale),
        ShimFreshness::Missing => outcomes.push(ClientOutcome::Failed),
    }
    match mcp {
        McpRegistration::Registered => outcomes.push(ClientOutcome::Registered),
        McpRegistration::Unsupported => outcomes.push(ClientOutcome::Unsupported),
        McpRegistration::Missing => {}
    }
    dedup_outcomes(&mut outcomes);
    ClientSetupEntry {
        tool,
        outcomes,
        shim,
        shim_path: SetupPath::project(tool.output_path(repo_root)),
        mcp,
        mcp_path: tool
            .mcp_config_relative_path()
            .map(|rel| SetupPath::project(repo_root.join(rel))),
        error: None,
    }
}

pub(crate) fn entry_after_failure(
    repo_root: &Path,
    tool: AgentTool,
    detected: bool,
    error: &anyhow::Error,
) -> ClientSetupEntry {
    let mut outcomes = Vec::new();
    push_if(&mut outcomes, detected, ClientOutcome::Detected);
    outcomes.push(ClientOutcome::Failed);
    ClientSetupEntry {
        tool,
        outcomes,
        shim: classify_shim_freshness(repo_root, tool),
        shim_path: SetupPath::project(tool.output_path(repo_root)),
        mcp: classify_mcp_registration(repo_root, tool),
        mcp_path: tool
            .mcp_config_relative_path()
            .map(|rel| SetupPath::project(repo_root.join(rel))),
        error: Some(format!("{error:#}")),
    }
}

pub(crate) fn classify_shim_freshness(repo_root: &Path, tool: AgentTool) -> ShimFreshness {
    let path = tool.output_path(repo_root);
    if !path.exists() {
        return ShimFreshness::Missing;
    }
    match std::fs::read_to_string(&path) {
        Ok(existing) if existing == tool.shim_content() => ShimFreshness::Current,
        Ok(_) | Err(_) => ShimFreshness::Stale,
    }
}

pub(crate) fn classify_mcp_registration(repo_root: &Path, tool: AgentTool) -> McpRegistration {
    if matches!(tool.automation_tier(), AutomationTier::ShimOnly) {
        return McpRegistration::Unsupported;
    }
    if mcp_registered(repo_root, tool) {
        McpRegistration::Registered
    } else {
        McpRegistration::Missing
    }
}

pub(crate) fn render_detected_client_summary(
    detected: &[AgentTool],
    selected: &[AgentTool],
    skipped: &[AgentTool],
) -> String {
    let mut out = String::new();
    out.push_str("Detected clients: ");
    out.push_str(&render_tool_list(detected));
    out.push('\n');
    out.push_str("Selected clients: ");
    out.push_str(&render_tool_list(selected));
    out.push('\n');
    if !skipped.is_empty() {
        out.push_str("Skipped clients: ");
        out.push_str(&render_tool_list(skipped));
        out.push('\n');
    }
    out
}

pub(crate) fn render_client_setup_summary(
    repo_root: &Path,
    kind: &str,
    entries: &[ClientSetupEntry],
) -> String {
    if entries.is_empty() {
        return String::new();
    }
    let mut out = format!("Client {kind} summary:\n");
    for entry in entries {
        let outcomes = entry
            .outcomes
            .iter()
            .map(|outcome| outcome.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!(
            "  - {} [{}]\n",
            entry.tool.display_name(),
            outcomes
        ));
        out.push_str(&format!(
            "    shim: {} ({})\n",
            entry.shim_path.render(repo_root),
            entry.shim.as_str()
        ));
        match &entry.mcp_path {
            Some(path) => out.push_str(&format!(
                "    mcp: {} ({})\n",
                path.render(repo_root),
                entry.mcp.as_str()
            )),
            None => out.push_str(&format!("    mcp: {}\n", entry.mcp.as_str())),
        }
        if entry.shim == ShimFreshness::Stale {
            out.push_str(&format!(
                "    next: run `synrepo agent-setup {} --regen` to refresh the shim\n",
                entry.tool.canonical_name()
            ));
        }
        if let Some(error) = &entry.error {
            out.push_str(&format!("    error: {error}\n"));
        }
    }
    out
}

fn render_tool_list(tools: &[AgentTool]) -> String {
    if tools.is_empty() {
        "none".to_string()
    } else {
        tools
            .iter()
            .map(|tool| tool.canonical_name())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn push_if(outcomes: &mut Vec<ClientOutcome>, condition: bool, outcome: ClientOutcome) {
    if condition {
        outcomes.push(outcome);
    }
}

fn dedup_outcomes(outcomes: &mut Vec<ClientOutcome>) {
    let mut seen = Vec::new();
    outcomes.retain(|outcome| {
        if seen.contains(outcome) {
            false
        } else {
            seen.push(*outcome);
            true
        }
    });
}

fn mcp_registered(repo_root: &Path, tool: AgentTool) -> bool {
    match tool {
        AgentTool::Claude => json_mcp_servers_has_synrepo(&repo_root.join(".mcp.json")),
        AgentTool::Cursor => json_mcp_servers_has_synrepo(&repo_root.join(".cursor/mcp.json")),
        AgentTool::Windsurf => json_mcp_servers_has_synrepo(&repo_root.join(".windsurf/mcp.json")),
        AgentTool::Roo => json_mcp_servers_has_synrepo(&repo_root.join(".roo/mcp.json")),
        AgentTool::OpenCode => opencode_has_synrepo(&repo_root.join("opencode.json")),
        AgentTool::Codex => codex_has_synrepo(&repo_root.join(".codex/config.toml")),
        AgentTool::Copilot
        | AgentTool::Generic
        | AgentTool::Gemini
        | AgentTool::Goose
        | AgentTool::Kiro
        | AgentTool::Qwen
        | AgentTool::Junie
        | AgentTool::Tabnine
        | AgentTool::Trae => false,
    }
}

fn json_mcp_servers_has_synrepo(path: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return false;
    };
    value
        .get("mcpServers")
        .and_then(|servers| servers.get("synrepo"))
        .is_some()
}

fn opencode_has_synrepo(path: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return false;
    };
    value
        .get("mcp")
        .and_then(|servers| servers.get("synrepo"))
        .is_some()
}

fn codex_has_synrepo(path: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };
    let Ok(doc) = text.parse::<toml_edit::DocumentMut>() else {
        return false;
    };
    doc.get("mcp_servers")
        .and_then(|item| item.as_table())
        .and_then(|table| table.get("synrepo"))
        .is_some()
}
