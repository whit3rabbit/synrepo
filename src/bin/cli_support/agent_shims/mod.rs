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
mod tool_impl;

#[cfg(test)]
mod tests;

pub(crate) use synrepo::agent_install::{SYNREPO_INSTALL_NAME, SYNREPO_INSTALL_OWNER};

pub(crate) fn scope_label(scope: &agent_config::Scope) -> &'static str {
    match scope {
        agent_config::Scope::Global => "global",
        agent_config::Scope::Local(_) => "project",
        _ => "global",
    }
}

/// Two-tier support matrix. `Automated` agents get the MCP server entry
/// written into their agent config by `synrepo setup` (global by default,
/// repo-local with `--project`). `ShimOnly` agents get their instruction file
/// written; MCP registration is documented but left to the operator, either
/// because the agent has no documented MCP config path or because the agent is
/// a second-wave target not worth automating yet.
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
    /// Sourcegraph Amp CLI — writes `.amp/skills/synrepo/SKILL.md`
    Amp,
    /// Google Antigravity — writes `.antigravity/skills/synrepo/SKILL.md`
    Antigravity,
    /// Claude Code — writes `.claude/skills/synrepo/SKILL.md`
    Claude,
    /// Cline — writes `.cline/skills/synrepo/SKILL.md`
    Cline,
    /// CodeBuddy CLI — writes `.codebuddy/skills/synrepo/SKILL.md`
    CodeBuddy,
    /// Cursor — writes `.cursor/skills/synrepo/SKILL.md`
    Cursor,
    /// GitHub Copilot — writes `synrepo-copilot-instructions.md`
    Copilot,
    /// Charm Crush — writes `.crush/skills/synrepo/SKILL.md`
    Crush,
    /// Generic AGENTS.md — writes `synrepo-agents.md`
    Generic,
    /// OpenAI Codex CLI — writes `.agents/skills/synrepo/SKILL.md`
    Codex,
    /// Forge — writes `.forge/skills/synrepo/SKILL.md`
    Forge,
    /// Hermes — writes `.hermes/skills/synrepo/SKILL.md`
    Hermes,
    /// iFlow CLI — writes MCP config only.
    Iflow,
    /// Junie — writes `.junie/commands/synrepo.md`
    Junie,
    /// Kilo Code — writes `.kilocode/skills/synrepo/SKILL.md`
    Kilocode,
    /// Windsurf — writes `.windsurf/skills/synrepo/SKILL.md`
    Windsurf,
    /// OpenCode — writes `AGENTS.md`
    OpenCode,
    /// OpenClaw — writes `.openclaw/skills/synrepo/SKILL.md`
    Openclaw,
    /// Pi — writes `.pi/skills/synrepo/SKILL.md`
    Pi,
    /// Google Gemini CLI — writes `.gemini/skills/synrepo/SKILL.md`
    Gemini,
    /// Goose — writes `.goose/recipes/synrepo.yaml`
    Goose,
    /// Kiro CLI — writes `.kiro/prompts/synrepo.md`
    Kiro,
    /// Qoder CLI — writes MCP config/instructions.
    Qodercli,
    /// Qwen Code — writes `.qwen/commands/synrepo.md`
    Qwen,
    /// Roo Code — writes `.roo/commands/synrepo.md`
    Roo,
    /// Tabnine CLI — writes `.tabnine/agent/commands/synrepo.toml`
    Tabnine,
    /// Trae — writes `.trae/skills/synrepo/SKILL.md`
    Trae,
}
