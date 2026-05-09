use std::path::{Path, PathBuf};

use agent_config::{InstallStatus, Scope};

use crate::cli_support::agent_shims::{
    AgentTool, AutomationTier, SYNREPO_INSTALL_NAME, SYNREPO_INSTALL_OWNER,
};

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
    fn new(scope: &Scope, path: PathBuf) -> Self {
        let scope = match scope {
            Scope::Global => SetupScope::Global,
            Scope::Local(_) => SetupScope::Project,
            _ => SetupScope::Global,
        };
        Self { scope, path }
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CodexSkillWarning {
    pub path: PathBuf,
    pub content_differs: bool,
    pub duplicate_frontmatter: bool,
}

impl ClientSetupEntry {
    pub(crate) fn skipped(
        repo_root: &Path,
        tool: AgentTool,
        detected: bool,
        scope: &Scope,
    ) -> Self {
        let mut outcomes = Vec::new();
        push_if(&mut outcomes, detected, ClientOutcome::Detected);
        outcomes.push(ClientOutcome::Skipped);
        Self {
            tool,
            outcomes,
            shim: classify_shim_freshness(repo_root, tool, scope),
            shim_path: SetupPath::new(scope, shim_path(repo_root, tool, scope)),
            mcp: classify_mcp_registration(repo_root, tool, scope),
            mcp_path: mcp_path(repo_root, tool, scope).map(|path| SetupPath::new(scope, path)),
            error: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ClientBefore {
    shim: ShimFreshness,
}

impl ClientBefore {
    pub(crate) fn observe(repo_root: &Path, tool: AgentTool, scope: &Scope) -> Self {
        Self {
            shim: classify_shim_freshness(repo_root, tool, scope),
        }
    }
}

pub(crate) fn entry_after_success(
    repo_root: &Path,
    tool: AgentTool,
    before: ClientBefore,
    detected: bool,
    scope: &Scope,
) -> ClientSetupEntry {
    let shim = classify_shim_freshness(repo_root, tool, scope);
    let mcp = classify_mcp_registration(repo_root, tool, scope);
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
        shim_path: SetupPath::new(scope, shim_path(repo_root, tool, scope)),
        mcp,
        mcp_path: mcp_path(repo_root, tool, scope).map(|path| SetupPath::new(scope, path)),
        error: None,
    }
}

pub(crate) fn entry_after_failure(
    repo_root: &Path,
    tool: AgentTool,
    detected: bool,
    scope: &Scope,
    error: &anyhow::Error,
) -> ClientSetupEntry {
    let mut outcomes = Vec::new();
    push_if(&mut outcomes, detected, ClientOutcome::Detected);
    outcomes.push(ClientOutcome::Failed);
    ClientSetupEntry {
        tool,
        outcomes,
        shim: classify_shim_freshness(repo_root, tool, scope),
        shim_path: SetupPath::new(scope, shim_path(repo_root, tool, scope)),
        mcp: classify_mcp_registration(repo_root, tool, scope),
        mcp_path: mcp_path(repo_root, tool, scope).map(|path| SetupPath::new(scope, path)),
        error: Some(format!("{error:#}")),
    }
}

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
        if entry.tool == AgentTool::Codex {
            for warning in codex_global_skill_warnings() {
                out.push_str(&format!(
                    "    warning: global Codex skill {} {}.\n",
                    warning.path.display(),
                    codex_skill_warning_reason(&warning)
                ));
                out.push_str(
                    "    next: run `synrepo setup codex --force` for global setup, or `synrepo agent-setup codex --regen` for a project-local refresh\n",
                );
            }
        }
        if let Some(error) = &entry.error {
            out.push_str(&format!("    error: {error}\n"));
        }
    }
    out
}

pub(crate) fn codex_global_skill_warnings() -> Vec<CodexSkillWarning> {
    let mut paths = Vec::new();
    if let Some(path) = AgentTool::Codex.resolved_shim_output_path(&Scope::Global) {
        paths.push(path);
    }
    if let Some(home) = std::env::var_os("HOME") {
        paths.push(
            PathBuf::from(home)
                .join(".agents")
                .join("skills")
                .join("synrepo")
                .join("SKILL.md"),
        );
    }
    paths.sort();
    paths.dedup();
    paths
        .into_iter()
        .filter_map(|path| codex_skill_warning_for_path(path))
        .collect()
}

fn codex_skill_warning_for_path(path: PathBuf) -> Option<CodexSkillWarning> {
    let existing = std::fs::read_to_string(&path).ok()?;
    codex_skill_warning_for_content(&existing).map(|(content_differs, duplicate_frontmatter)| {
        CodexSkillWarning {
            path,
            content_differs,
            duplicate_frontmatter,
        }
    })
}

fn codex_skill_warning_for_content(content: &str) -> Option<(bool, bool)> {
    let content_differs = content != AgentTool::Codex.shim_content();
    let duplicate_frontmatter = has_duplicate_frontmatter(content);
    if content_differs || duplicate_frontmatter {
        Some((content_differs, duplicate_frontmatter))
    } else {
        None
    }
}

fn has_duplicate_frontmatter(content: &str) -> bool {
    content
        .lines()
        .filter(|line| line.trim() == "---")
        .take(4)
        .count()
        >= 4
}

fn codex_skill_warning_reason(warning: &CodexSkillWarning) -> &'static str {
    match (warning.content_differs, warning.duplicate_frontmatter) {
        (true, true) => "differs from the generated shim and has duplicate frontmatter",
        (true, false) => "differs from the generated shim",
        (false, true) => "has duplicate frontmatter",
        (false, false) => "needs review",
    }
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

fn shim_path(repo_root: &Path, tool: AgentTool, scope: &Scope) -> PathBuf {
    tool.resolved_shim_output_path(scope)
        .unwrap_or_else(|| tool.output_path(repo_root))
}

fn mcp_path(repo_root: &Path, tool: AgentTool, scope: &Scope) -> Option<PathBuf> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_skill_warning_detects_duplicate_frontmatter() {
        let content = format!(
            "---\nname: synrepo\n---\n{}",
            AgentTool::Codex.shim_content()
        );
        let warning = codex_skill_warning_for_content(&content).expect("warning");

        assert_eq!(warning, (true, true));
    }

    #[test]
    fn codex_skill_warning_accepts_current_generated_skill() {
        assert!(codex_skill_warning_for_content(AgentTool::Codex.shim_content()).is_none());
    }

    #[test]
    fn setup_summary_reports_stale_global_codex_skill() {
        let home = tempfile::tempdir().unwrap();
        let _home_guard = synrepo::config::test_home::HomeEnvGuard::redirect_to(home.path());
        let skill_path = home
            .path()
            .join(".agents")
            .join("skills")
            .join("synrepo")
            .join("SKILL.md");
        std::fs::create_dir_all(skill_path.parent().unwrap()).unwrap();
        std::fs::write(
            &skill_path,
            format!(
                "---\nname: synrepo\n---\n{}",
                AgentTool::Codex.shim_content()
            ),
        )
        .unwrap();
        let repo = tempfile::tempdir().unwrap();
        let entry = ClientSetupEntry {
            tool: AgentTool::Codex,
            outcomes: vec![ClientOutcome::Skipped],
            shim: ShimFreshness::Current,
            shim_path: SetupPath {
                scope: SetupScope::Project,
                path: repo.path().join(".agents/skills/synrepo/SKILL.md"),
            },
            mcp: McpRegistration::Missing,
            mcp_path: None,
            error: None,
        };

        let summary = render_client_setup_summary(repo.path(), "setup", &[entry]);

        assert!(summary.contains("global Codex skill"), "{summary}");
        assert!(summary.contains("duplicate frontmatter"), "{summary}");
        assert!(summary.contains("synrepo setup codex --force"), "{summary}");
        assert!(
            summary.contains("synrepo agent-setup codex --regen"),
            "{summary}"
        );
    }
}
