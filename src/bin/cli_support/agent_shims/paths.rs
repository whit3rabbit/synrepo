use std::path::{Path, PathBuf};

use synrepo::agent_install::skill_manifest_path;

use super::{AgentTool, ShimPlacement, SYNREPO_INSTALL_NAME, SYNREPO_INSTALL_OWNER};

impl AgentTool {
    /// Path of the file written by `synrepo agent-setup`, relative to the repo root.
    ///
    /// For agent-config backed surfaces this first asks the installer for its
    /// local-scope status path, then falls back to the legacy synrepo-local
    /// table. Keeping the fallback lets older runtime probes and legacy remove
    /// scans keep working while agent-config is the source of truth for
    /// managed installs.
    pub(crate) fn output_path(self, repo_root: &Path) -> PathBuf {
        let scope = agent_config::Scope::Local(repo_root.to_path_buf());
        self.resolved_shim_output_path(&scope)
            .unwrap_or_else(|| self.legacy_output_path(repo_root))
    }

    pub(crate) fn resolved_shim_output_path(self, scope: &agent_config::Scope) -> Option<PathBuf> {
        let id = self.agent_config_id()?;
        match self.placement_kind() {
            ShimPlacement::Skill { name } => {
                let installer = agent_config::skill_by_id(id)?;
                let report = installer
                    .skill_status(scope, name, SYNREPO_INSTALL_OWNER)
                    .ok()?;
                skill_manifest_path(report)
            }
            ShimPlacement::Instruction { name, .. } => {
                let installer = agent_config::instruction_by_id(id)?;
                installer
                    .instruction_status(scope, name, SYNREPO_INSTALL_OWNER)
                    .ok()
                    .and_then(|report| report.config_path)
            }
            ShimPlacement::Local => None,
        }
    }

    pub(crate) fn resolved_mcp_config_path(self, scope: &agent_config::Scope) -> Option<PathBuf> {
        let id = self.agent_config_id()?;
        let installer = agent_config::mcp_by_id(id)?;
        installer
            .mcp_status(scope, SYNREPO_INSTALL_NAME, SYNREPO_INSTALL_OWNER)
            .ok()
            .and_then(|report| report.config_path)
    }

    fn legacy_output_path(self, repo_root: &Path) -> PathBuf {
        match self {
            AgentTool::Amp => repo_root
                .join(".amp")
                .join("skills")
                .join("synrepo")
                .join("SKILL.md"),
            AgentTool::Antigravity => repo_root
                .join(".antigravity")
                .join("skills")
                .join("synrepo")
                .join("SKILL.md"),
            AgentTool::Claude => repo_root
                .join(".claude")
                .join("skills")
                .join("synrepo")
                .join("SKILL.md"),
            AgentTool::Cline => repo_root
                .join(".cline")
                .join("skills")
                .join("synrepo")
                .join("SKILL.md"),
            AgentTool::CodeBuddy => repo_root
                .join(".codebuddy")
                .join("skills")
                .join("synrepo")
                .join("SKILL.md"),
            AgentTool::Cursor => repo_root
                .join(".cursor")
                .join("skills")
                .join("synrepo")
                .join("SKILL.md"),
            AgentTool::Copilot => repo_root.join("synrepo-copilot-instructions.md"),
            AgentTool::Crush => repo_root
                .join(".crush")
                .join("skills")
                .join("synrepo")
                .join("SKILL.md"),
            AgentTool::Generic => repo_root.join("synrepo-agents.md"),
            AgentTool::Codex => repo_root
                .join(".agents")
                .join("skills")
                .join("synrepo")
                .join("SKILL.md"),
            AgentTool::Forge => repo_root
                .join(".forge")
                .join("skills")
                .join("synrepo")
                .join("SKILL.md"),
            AgentTool::Hermes => repo_root
                .join(".hermes")
                .join("skills")
                .join("synrepo")
                .join("SKILL.md"),
            AgentTool::Iflow => repo_root.join(".iflow").join("mcp.json"),
            AgentTool::Junie => repo_root.join(".junie").join("commands").join("synrepo.md"),
            AgentTool::Kilocode => repo_root
                .join(".kilocode")
                .join("skills")
                .join("synrepo")
                .join("SKILL.md"),
            AgentTool::Windsurf => repo_root
                .join(".windsurf")
                .join("skills")
                .join("synrepo")
                .join("SKILL.md"),
            AgentTool::OpenCode => repo_root.join("AGENTS.md"),
            AgentTool::Openclaw => repo_root
                .join(".openclaw")
                .join("skills")
                .join("synrepo")
                .join("SKILL.md"),
            AgentTool::Pi => repo_root
                .join(".pi")
                .join("skills")
                .join("synrepo")
                .join("SKILL.md"),
            AgentTool::Gemini => repo_root
                .join(".gemini")
                .join("skills")
                .join("synrepo")
                .join("SKILL.md"),
            AgentTool::Goose => repo_root
                .join(".goose")
                .join("recipes")
                .join("synrepo.yaml"),
            AgentTool::Kiro => repo_root.join(".kiro").join("prompts").join("synrepo.md"),
            AgentTool::Qodercli => repo_root.join(".qoder").join("mcp.json"),
            AgentTool::Qwen => repo_root.join(".qwen").join("commands").join("synrepo.md"),
            AgentTool::Roo => repo_root.join(".roo").join("commands").join("synrepo.md"),
            AgentTool::Tabnine => repo_root
                .join(".tabnine")
                .join("agent")
                .join("commands")
                .join("synrepo.toml"),
            AgentTool::Trae => repo_root
                .join(".trae")
                .join("skills")
                .join("synrepo")
                .join("SKILL.md"),
        }
    }

    /// User-facing noun for the artifact written by `agent-setup`.
    pub(crate) fn artifact_label(self) -> &'static str {
        match self.placement_kind() {
            ShimPlacement::Skill { .. } => "skill",
            ShimPlacement::Instruction { .. } | ShimPlacement::Local => "instructions",
        }
    }
}
