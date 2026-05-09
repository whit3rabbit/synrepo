mod codex_warnings;
mod render;
mod status;

use std::path::{Path, PathBuf};

use agent_config::Scope;

use crate::cli_support::agent_shims::AgentTool;

pub(crate) use render::{render_client_setup_summary, render_detected_client_summary};
pub(crate) use status::{classify_mcp_registration, classify_shim_freshness};

use status::{mcp_path, shim_path};

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

#[cfg(test)]
mod tests {
    use super::codex_warnings::codex_skill_warning_for_content;
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
