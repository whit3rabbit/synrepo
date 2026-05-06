use agent_config::{InstructionPlacement, ScopeKind};
use clap::ValueEnum;

use super::shims::{
    CLAUDE_SHIM, CODEX_SHIM, COPILOT_SHIM, CURSOR_SHIM, GEMINI_SHIM, GENERIC_SHIM, GOOSE_SHIM,
    JUNIE_SHIM, KIRO_SHIM, OPENCODE_SHIM, QWEN_SHIM, ROO_SHIM, TABNINE_SHIM, TRAE_SHIM,
    WINDSURF_SHIM,
};
use super::{AgentTool, AutomationTier, ShimPlacement, SYNREPO_INSTALL_NAME};

impl AgentTool {
    pub(crate) fn from_target_kind(
        kind: synrepo::bootstrap::runtime_probe::AgentTargetKind,
    ) -> Self {
        use synrepo::bootstrap::runtime_probe::AgentTargetKind;
        match kind {
            AgentTargetKind::Amp => AgentTool::Amp,
            AgentTargetKind::Antigravity => AgentTool::Antigravity,
            AgentTargetKind::Claude => AgentTool::Claude,
            AgentTargetKind::Cline => AgentTool::Cline,
            AgentTargetKind::CodeBuddy => AgentTool::CodeBuddy,
            AgentTargetKind::Cursor => AgentTool::Cursor,
            AgentTargetKind::Copilot => AgentTool::Copilot,
            AgentTargetKind::Crush => AgentTool::Crush,
            AgentTargetKind::Codex => AgentTool::Codex,
            AgentTargetKind::Forge => AgentTool::Forge,
            AgentTargetKind::Gemini => AgentTool::Gemini,
            AgentTargetKind::Hermes => AgentTool::Hermes,
            AgentTargetKind::Iflow => AgentTool::Iflow,
            AgentTargetKind::Junie => AgentTool::Junie,
            AgentTargetKind::Kilocode => AgentTool::Kilocode,
            AgentTargetKind::Opencode => AgentTool::OpenCode,
            AgentTargetKind::Openclaw => AgentTool::Openclaw,
            AgentTargetKind::Pi => AgentTool::Pi,
            AgentTargetKind::Qodercli => AgentTool::Qodercli,
            AgentTargetKind::Qwen => AgentTool::Qwen,
            AgentTargetKind::Roo => AgentTool::Roo,
            AgentTargetKind::Tabnine => AgentTool::Tabnine,
            AgentTargetKind::Trae => AgentTool::Trae,
            AgentTargetKind::Windsurf => AgentTool::Windsurf,
        }
    }

    pub(crate) fn from_agent_config_id(id: &str) -> Option<Self> {
        Self::value_variants()
            .iter()
            .copied()
            .find(|tool| tool.agent_config_id() == Some(id))
    }

    pub(crate) fn local_mcp_tools() -> Vec<Self> {
        agent_config::mcp_capable()
            .into_iter()
            .filter(|installer| installer.supported_mcp_scopes().contains(&ScopeKind::Local))
            .filter_map(|installer| Self::from_agent_config_id(installer.id()))
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn local_artifact_tools() -> Vec<Self> {
        Self::value_variants()
            .iter()
            .copied()
            .filter(|tool| {
                let Some(id) = tool.agent_config_id() else {
                    return false;
                };
                agent_config::skill_by_id(id)
                    .map(|s| s.supported_skill_scopes().contains(&ScopeKind::Local))
                    .unwrap_or(false)
                    || agent_config::instruction_by_id(id)
                        .map(|i| i.supported_instruction_scopes().contains(&ScopeKind::Local))
                        .unwrap_or(false)
            })
            .collect()
    }

    pub(crate) fn automation_tier(self) -> AutomationTier {
        if self.installer_supports_mcp() {
            AutomationTier::Automated
        } else {
            AutomationTier::ShimOnly
        }
    }

    pub(crate) fn agent_config_id(self) -> Option<&'static str> {
        match self {
            AgentTool::Amp => Some("amp"),
            AgentTool::Antigravity => Some("antigravity"),
            AgentTool::Claude => Some("claude"),
            AgentTool::Cline => Some("cline"),
            AgentTool::CodeBuddy => Some("codebuddy"),
            AgentTool::Cursor => Some("cursor"),
            AgentTool::Copilot => Some("copilot"),
            AgentTool::Crush => Some("crush"),
            AgentTool::Generic => None,
            AgentTool::Codex => Some("codex"),
            AgentTool::Forge => Some("forge"),
            AgentTool::Hermes => Some("hermes"),
            AgentTool::Iflow => Some("iflow"),
            AgentTool::Junie => Some("junie"),
            AgentTool::Kilocode => Some("kilocode"),
            AgentTool::Windsurf => Some("windsurf"),
            AgentTool::OpenCode => Some("opencode"),
            AgentTool::Openclaw => Some("openclaw"),
            AgentTool::Pi => Some("pi"),
            AgentTool::Gemini => Some("gemini"),
            AgentTool::Goose => None,
            AgentTool::Kiro => None,
            AgentTool::Qodercli => Some("qodercli"),
            AgentTool::Qwen => Some("qwen"),
            AgentTool::Roo => Some("roo"),
            AgentTool::Tabnine => Some("tabnine"),
            AgentTool::Trae => Some("trae"),
        }
    }

    pub(crate) fn installer_supports_mcp(self) -> bool {
        self.agent_config_id()
            .and_then(agent_config::mcp_by_id)
            .is_some()
    }

    pub(crate) fn supported_scopes(self) -> &'static [ScopeKind] {
        self.agent_config_id()
            .and_then(agent_config::mcp_by_id)
            .map(|installer| installer.supported_mcp_scopes())
            .unwrap_or(&[])
    }

    pub(crate) fn placement_kind(self) -> ShimPlacement {
        let Some(id) = self.agent_config_id() else {
            return ShimPlacement::Local;
        };
        if agent_config::skill_by_id(id).is_some() {
            return ShimPlacement::Skill {
                name: SYNREPO_INSTALL_NAME,
            };
        }
        if agent_config::instruction_by_id(id).is_some() {
            return ShimPlacement::Instruction {
                name: SYNREPO_INSTALL_NAME,
                placement: self.instruction_placement(),
            };
        }
        ShimPlacement::Local
    }

    fn instruction_placement(self) -> InstructionPlacement {
        match self {
            AgentTool::Antigravity
            | AgentTool::Cline
            | AgentTool::Kilocode
            | AgentTool::Roo
            | AgentTool::Windsurf => InstructionPlacement::StandaloneFile,
            _ => InstructionPlacement::InlineBlock,
        }
    }

    pub(crate) fn skill_description(self) -> &'static str {
        "Use when a repository has synrepo context available."
    }

    pub(crate) fn shim_spec_body(self) -> &'static str {
        self.shim_content()
    }

    pub(crate) fn display_name(self) -> &'static str {
        match self {
            AgentTool::Amp => "Amp",
            AgentTool::Antigravity => "Google Antigravity",
            AgentTool::Claude => "Claude Code",
            AgentTool::Cline => "Cline",
            AgentTool::CodeBuddy => "CodeBuddy CLI",
            AgentTool::Cursor => "Cursor",
            AgentTool::Copilot => "GitHub Copilot",
            AgentTool::Crush => "Charm Crush",
            AgentTool::Generic => "generic (AGENTS.md)",
            AgentTool::Codex => "Codex CLI",
            AgentTool::Forge => "Forge",
            AgentTool::Hermes => "Hermes",
            AgentTool::Iflow => "iFlow CLI",
            AgentTool::Junie => "Junie",
            AgentTool::Kilocode => "Kilo Code",
            AgentTool::Windsurf => "Windsurf",
            AgentTool::OpenCode => "OpenCode",
            AgentTool::Openclaw => "OpenClaw",
            AgentTool::Pi => "Pi",
            AgentTool::Gemini => "Gemini CLI",
            AgentTool::Goose => "Goose",
            AgentTool::Kiro => "Kiro CLI",
            AgentTool::Qodercli => "Qoder CLI",
            AgentTool::Qwen => "Qwen Code",
            AgentTool::Roo => "Roo Code",
            AgentTool::Tabnine => "Tabnine CLI",
            AgentTool::Trae => "Trae",
        }
    }

    pub(crate) fn include_instruction(self) -> &'static str {
        match self {
            AgentTool::Claude => {
                "Claude Code auto-discovers `.claude/skills/synrepo/SKILL.md` on startup; no further action required."
            }
            AgentTool::Cursor => {
                "Cursor 2.4+ auto-discovers the synrepo skill on startup."
            }
            AgentTool::Copilot => {
                "Paste the contents of `synrepo-copilot-instructions.md` into `.github/copilot-instructions.md`."
            }
            AgentTool::Codex => "Codex CLI auto-discovers the synrepo skill on startup.",
            AgentTool::Windsurf => "Windsurf auto-discovers the synrepo skill on startup.",
            AgentTool::OpenCode => "OpenCode loads `AGENTS.md` as a project rule automatically.",
            AgentTool::Gemini => {
                "Gemini CLI auto-discovers `.gemini/skills/synrepo/SKILL.md`; register `synrepo mcp --repo .` as a stdio MCP server in `.gemini/settings.json` yourself."
            }
            AgentTool::Qwen => {
                "Write the synrepo config manually: create `.qwen/commands/synrepo.md`."
            }
            AgentTool::Junie => {
                "Write the synrepo config manually: create `.junie/commands/synrepo.md`."
            }
            AgentTool::Roo => "Roo Code loads .roo/commands/synrepo.md automatically.",
            AgentTool::Tabnine => {
                "Write the synrepo config manually: create `.tabnine/agent/commands/synrepo.toml`."
            }
            AgentTool::Trae => {
                "Trae loads `.trae/skills/synrepo/SKILL.md` as a skill automatically."
            }
            AgentTool::Generic => {
                "Paste the contents of `synrepo-agents.md` into your `AGENTS.md` file."
            }
            AgentTool::Goose => {
                "Write the synrepo config manually: create `.goose/recipes/synrepo.yaml`."
            }
            AgentTool::Kiro => "Kiro loads `.kiro/prompts/synrepo.md` as a prompt automatically.",
            _ if self.installer_supports_mcp() => {
                "synrepo MCP is managed through agent-config; restart the target client if it was already running."
            }
            _ => {
                "synrepo instructions are managed through agent-config; restart the target client if it was already running."
            }
        }
    }

    pub(crate) fn canonical_name(self) -> &'static str {
        match self {
            AgentTool::Amp => "amp",
            AgentTool::Antigravity => "antigravity",
            AgentTool::Claude => "claude",
            AgentTool::Cline => "cline",
            AgentTool::CodeBuddy => "code-buddy",
            AgentTool::Cursor => "cursor",
            AgentTool::Copilot => "copilot",
            AgentTool::Crush => "crush",
            AgentTool::Generic => "generic",
            AgentTool::Codex => "codex",
            AgentTool::Forge => "forge",
            AgentTool::Hermes => "hermes",
            AgentTool::Iflow => "iflow",
            AgentTool::Junie => "junie",
            AgentTool::Kilocode => "kilocode",
            AgentTool::Windsurf => "windsurf",
            AgentTool::OpenCode => "open-code",
            AgentTool::Openclaw => "openclaw",
            AgentTool::Pi => "pi",
            AgentTool::Gemini => "gemini",
            AgentTool::Goose => "goose",
            AgentTool::Kiro => "kiro",
            AgentTool::Qodercli => "qodercli",
            AgentTool::Qwen => "qwen",
            AgentTool::Roo => "roo",
            AgentTool::Tabnine => "tabnine",
            AgentTool::Trae => "trae",
        }
    }

    pub(crate) fn mcp_config_relative_path(self) -> Option<&'static str> {
        match self {
            AgentTool::Claude => Some(".mcp.json"),
            AgentTool::Codex => Some(".codex/config.toml"),
            AgentTool::OpenCode => Some("opencode.json"),
            AgentTool::Cursor => Some(".cursor/mcp.json"),
            AgentTool::Windsurf => Some(".windsurf/mcp.json"),
            AgentTool::Roo => Some(".roo/mcp.json"),
            _ => None,
        }
    }

    pub(crate) fn shim_content(self) -> &'static str {
        match self {
            AgentTool::Claude => CLAUDE_SHIM,
            AgentTool::Cursor => CURSOR_SHIM,
            AgentTool::Copilot => COPILOT_SHIM,
            AgentTool::Generic => GENERIC_SHIM,
            AgentTool::Codex => CODEX_SHIM,
            AgentTool::Windsurf => WINDSURF_SHIM,
            AgentTool::OpenCode => OPENCODE_SHIM,
            AgentTool::Gemini => GEMINI_SHIM,
            AgentTool::Goose => GOOSE_SHIM,
            AgentTool::Kiro => KIRO_SHIM,
            AgentTool::Qwen => QWEN_SHIM,
            AgentTool::Junie => JUNIE_SHIM,
            AgentTool::Roo => ROO_SHIM,
            AgentTool::Tabnine => TABNINE_SHIM,
            AgentTool::Trae => TRAE_SHIM,
            _ => CLAUDE_SHIM,
        }
    }
}
