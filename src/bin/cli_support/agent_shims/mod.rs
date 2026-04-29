//! Static shim content and output paths for `synrepo agent-setup <tool>`.
//!
//! Each shim teaches an agent how to use synrepo. Shims cover the canonical
//! agent doctrine (embedded verbatim from [`doctrine::DOCTRINE_BLOCK`]), the
//! MCP tool reference, and the CLI fallback. Long-form prose and worked
//! examples live in `skill/SKILL.md`; the shim is the minimum kit an agent
//! needs to use synrepo correctly without reading SKILL.md.

pub(crate) mod doctrine;
mod paths;
pub(crate) mod registry;
mod shims;

#[cfg(test)]
mod tests;

use shims::{
    CLAUDE_SHIM, CODEX_SHIM, COPILOT_SHIM, CURSOR_SHIM, GEMINI_SHIM, GENERIC_SHIM, GOOSE_SHIM,
    JUNIE_SHIM, KIRO_SHIM, OPENCODE_SHIM, QWEN_SHIM, ROO_SHIM, TABNINE_SHIM, TRAE_SHIM,
    WINDSURF_SHIM,
};

pub(crate) use synrepo::agent_install::{SYNREPO_INSTALL_NAME, SYNREPO_INSTALL_OWNER};

pub(crate) fn scope_label(scope: &agent_config::Scope) -> &'static str {
    match scope {
        agent_config::Scope::Global => "global",
        agent_config::Scope::Local(_) => "project",
        _ => "global",
    }
}

/// Two-tier support matrix. `Automated` agents get the project-scoped MCP
/// server entry written into their config (`.mcp.json`, `.codex/config.toml`,
/// or `opencode.json`) by `synrepo setup`. `ShimOnly` agents get their
/// instruction file written; MCP registration is documented but left to the
/// operator, either because the agent has no documented project-scoped MCP
/// config path or because the agent is a second-wave target not worth
/// automating yet.
///
/// The dispatch in `step_register_mcp` must agree with this tier assignment;
/// the `automation_tier_matches_step_register_mcp_dispatch` test is the
/// anti-drift guard.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AutomationTier {
    /// `synrepo setup` writes the MCP-server entry into the agent's config.
    Automated,
    /// `synrepo setup` writes the shim only; the operator wires MCP by hand.
    ShimOnly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShimPlacement {
    Skill {
        name: &'static str,
    },
    Instruction {
        name: &'static str,
        placement: agent_config::InstructionPlacement,
    },
    /// Synrepo-local fallback for targets not yet covered by agent-config.
    /// Today this covers `Generic`, `Goose`, `Kiro`, and `Tabnine`.
    Local,
}

/// Agent CLI target for shim generation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, clap::ValueEnum)]
pub(crate) enum AgentTool {
    /// Claude Code — writes `.claude/skills/synrepo/SKILL.md`
    Claude,
    /// Cursor — writes `.cursor/skills/synrepo/SKILL.md`
    Cursor,
    /// GitHub Copilot — writes `synrepo-copilot-instructions.md`
    Copilot,
    /// Generic AGENTS.md — writes `synrepo-agents.md`
    Generic,
    /// OpenAI Codex CLI — writes `.agents/skills/synrepo/SKILL.md`
    Codex,
    /// Windsurf — writes `.windsurf/skills/synrepo/SKILL.md`
    Windsurf,
    /// OpenCode — writes `AGENTS.md`
    OpenCode,
    /// Google Gemini CLI — writes `.gemini/skills/synrepo/SKILL.md`
    Gemini,
    /// Goose — writes `.goose/recipes/synrepo.yaml`
    Goose,
    /// Kiro CLI — writes `.kiro/prompts/synrepo.md`
    Kiro,
    /// Qwen Code — writes `.qwen/commands/synrepo.md`
    Qwen,
    /// Junie — writes `.junie/commands/synrepo.md`
    Junie,
    /// Roo Code — writes `.roo/commands/synrepo.md`
    Roo,
    /// Tabnine CLI — writes `.tabnine/agent/commands/synrepo.toml`
    Tabnine,
    /// Trae — writes `.trae/skills/synrepo/SKILL.md`
    Trae,
}

impl AgentTool {
    /// Translate an [`AgentTargetKind`] from the library probe into the
    /// binary's wider [`AgentTool`] enum. Used by the setup wizard when
    /// executing its plan — the library offers the observationally-detectable
    /// subset, and the binary owns the full shim roster.
    pub(crate) fn from_target_kind(
        kind: synrepo::bootstrap::runtime_probe::AgentTargetKind,
    ) -> Self {
        use synrepo::bootstrap::runtime_probe::AgentTargetKind;
        match kind {
            AgentTargetKind::Claude => AgentTool::Claude,
            AgentTargetKind::Cursor => AgentTool::Cursor,
            AgentTargetKind::Codex => AgentTool::Codex,
            AgentTargetKind::Copilot => AgentTool::Copilot,
            AgentTargetKind::Windsurf => AgentTool::Windsurf,
        }
    }

    /// Which support tier this agent falls into. See [`AutomationTier`].
    /// MCP automation now follows the agent-config registry so adding support
    /// upstream does not require another synrepo dispatch list.
    pub(crate) fn automation_tier(self) -> AutomationTier {
        if self.installer_supports_mcp() {
            AutomationTier::Automated
        } else {
            AutomationTier::ShimOnly
        }
    }

    /// Stable agent-config integration id for this target. `Generic` and
    /// harnesses not yet covered by agent-config stay synrepo-local.
    pub(crate) fn agent_config_id(self) -> Option<&'static str> {
        match self {
            AgentTool::Claude => Some("claude"),
            AgentTool::Cursor => Some("cursor"),
            AgentTool::Copilot => Some("copilot"),
            AgentTool::Generic => None,
            AgentTool::Codex => Some("codex"),
            AgentTool::Windsurf => Some("windsurf"),
            AgentTool::OpenCode => Some("opencode"),
            AgentTool::Gemini => Some("gemini"),
            AgentTool::Goose => None,
            AgentTool::Kiro => None,
            AgentTool::Qwen => Some("qwen"),
            AgentTool::Junie => Some("junie"),
            AgentTool::Roo => Some("roo"),
            AgentTool::Tabnine => Some("tabnine"),
            AgentTool::Trae => Some("trae"),
        }
    }

    /// True when the agent-config registry has an MCP installer for this tool.
    pub(crate) fn installer_supports_mcp(self) -> bool {
        self.agent_config_id()
            .and_then(agent_config::mcp_by_id)
            .is_some()
    }

    /// MCP scopes supported by the registered installer.
    pub(crate) fn supported_scopes(self) -> &'static [agent_config::ScopeKind] {
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

    fn instruction_placement(self) -> agent_config::InstructionPlacement {
        match self {
            AgentTool::Roo => agent_config::InstructionPlacement::StandaloneFile,
            _ => agent_config::InstructionPlacement::InlineBlock,
        }
    }

    pub(crate) fn skill_description(self) -> &'static str {
        "Use when a repository has synrepo context available."
    }

    pub(crate) fn shim_spec_body(self) -> &'static str {
        self.shim_content()
    }

    /// Human-readable name for display.
    pub(crate) fn display_name(self) -> &'static str {
        match self {
            AgentTool::Claude => "Claude Code",
            AgentTool::Cursor => "Cursor",
            AgentTool::Copilot => "GitHub Copilot",
            AgentTool::Generic => "generic (AGENTS.md)",
            AgentTool::Codex => "Codex CLI",
            AgentTool::Windsurf => "Windsurf",
            AgentTool::OpenCode => "OpenCode",
            AgentTool::Gemini => "Gemini CLI",
            AgentTool::Goose => "Goose",
            AgentTool::Kiro => "Kiro CLI",
            AgentTool::Qwen => "Qwen Code",
            AgentTool::Junie => "Junie",
            AgentTool::Roo => "Roo Code",
            AgentTool::Tabnine => "Tabnine CLI",
            AgentTool::Trae => "Trae",
        }
    }

    /// One-line instruction printed after the file is written.
    pub(crate) fn include_instruction(self) -> &'static str {
        match self {
            AgentTool::Claude => {
                "Claude Code auto-discovers `.claude/skills/synrepo/SKILL.md` on startup; no further action required."
            }
            AgentTool::Cursor => {
                "Cursor 2.4+ auto-discovers `.cursor/skills/synrepo/SKILL.md`; MCP server is registered in .cursor/mcp.json."
            }
            AgentTool::Copilot => {
                "Paste the contents of `synrepo-copilot-instructions.md` into \
                `.github/copilot-instructions.md`."
            }
            AgentTool::Generic => {
                "Paste the contents of `synrepo-agents.md` into your `AGENTS.md` file."
            }
            AgentTool::Codex => {
                "Codex CLI auto-discovers `.agents/skills/synrepo/SKILL.md`; MCP server is registered in project .codex/config.toml for trusted projects."
            }
            AgentTool::Windsurf => {
                "Windsurf auto-discovers `.windsurf/skills/synrepo/SKILL.md`; MCP server is registered in .windsurf/mcp.json."
            }
            AgentTool::OpenCode => "OpenCode loads `AGENTS.md` as a project rule automatically.",
            AgentTool::Gemini => {
                "Gemini CLI auto-discovers `.gemini/skills/synrepo/SKILL.md`; register `synrepo mcp --repo .` as a stdio MCP server in `.gemini/settings.json` yourself."
            }
            AgentTool::Goose => {
                "Write the synrepo config manually: create `.goose/recipes/synrepo.yaml`."
            }
            AgentTool::Kiro => "Kiro loads `.kiro/prompts/synrepo.md` as a prompt automatically.",
            AgentTool::Qwen => {
                "Write the synrepo config manually: create `.qwen/commands/synrepo.md`."
            }
            AgentTool::Junie => {
                "Write the synrepo config manually: create `.junie/commands/synrepo.md`."
            }
            AgentTool::Roo => {
                "Roo Code loads .roo/commands/synrepo.md automatically. The MCP server is registered in .roo/mcp.json."
            }
            AgentTool::Tabnine => {
                "Write the synrepo config manually: create `.tabnine/agent/commands/synrepo.toml`."
            }
            AgentTool::Trae => {
                "Trae loads `.trae/skills/synrepo/SKILL.md` as a skill automatically."
            }
        }
    }

    /// Stable, machine-readable name matching the CLI kebab-case form
    /// (e.g. `OpenCode` → `"open-code"`). Used as the `tool` key in the install
    /// registry at `~/.synrepo/projects.toml` so remove can map tool strings
    /// back to `AgentTool` variants deterministically.
    pub(crate) fn canonical_name(self) -> &'static str {
        match self {
            AgentTool::Claude => "claude",
            AgentTool::Cursor => "cursor",
            AgentTool::Copilot => "copilot",
            AgentTool::Generic => "generic",
            AgentTool::Codex => "codex",
            AgentTool::Windsurf => "windsurf",
            AgentTool::OpenCode => "open-code",
            AgentTool::Gemini => "gemini",
            AgentTool::Goose => "goose",
            AgentTool::Kiro => "kiro",
            AgentTool::Qwen => "qwen",
            AgentTool::Junie => "junie",
            AgentTool::Roo => "roo",
            AgentTool::Tabnine => "tabnine",
            AgentTool::Trae => "trae",
        }
    }

    /// Project-root-relative path of the MCP config file this tool edits
    /// during `synrepo setup`. Returns `None` for shim-only-tier tools.
    ///
    /// Kept for legacy project-local scans and backup prompts. New installs
    /// use the agent-config report path instead.
    pub(crate) fn mcp_config_relative_path(self) -> Option<&'static str> {
        match self {
            AgentTool::Claude => Some(".mcp.json"),
            AgentTool::Codex => Some(".codex/config.toml"),
            AgentTool::OpenCode => Some("opencode.json"),
            AgentTool::Cursor => Some(".cursor/mcp.json"),
            AgentTool::Windsurf => Some(".windsurf/mcp.json"),
            AgentTool::Roo => Some(".roo/mcp.json"),
            AgentTool::Copilot
            | AgentTool::Generic
            | AgentTool::Gemini
            | AgentTool::Goose
            | AgentTool::Kiro
            | AgentTool::Qwen
            | AgentTool::Junie
            | AgentTool::Tabnine
            | AgentTool::Trae => None,
        }
    }

    /// Static shim content for this target.
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
        }
    }
}
