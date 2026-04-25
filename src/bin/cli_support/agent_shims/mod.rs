//! Static shim content and output paths for `synrepo agent-setup <tool>`.
//!
//! Each shim teaches an agent how to use synrepo. Shims cover the canonical
//! agent doctrine (embedded verbatim from [`doctrine::DOCTRINE_BLOCK`]), the
//! MCP tool reference, and the CLI fallback. Long-form prose and worked
//! examples live in `skill/SKILL.md`; the shim is the minimum kit an agent
//! needs to use synrepo correctly without reading SKILL.md.

use std::path::{Path, PathBuf};

pub(crate) mod doctrine;
pub(crate) mod registry;
mod shims;

#[cfg(test)]
mod tests;

use shims::{
    CLAUDE_SHIM, CODEX_SHIM, COPILOT_SHIM, CURSOR_SHIM, GEMINI_SHIM, GENERIC_SHIM, GOOSE_SHIM,
    JUNIE_SHIM, KIRO_SHIM, OPENCODE_SHIM, QWEN_SHIM, ROO_SHIM, TABNINE_SHIM, TRAE_SHIM,
    WINDSURF_SHIM,
};

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
    /// The exhaustive match is load-bearing: any new `AgentTool` variant
    /// fails the build until it is explicitly placed in a tier.
    pub(crate) fn automation_tier(self) -> AutomationTier {
        match self {
            AgentTool::Claude
            | AgentTool::Codex
            | AgentTool::OpenCode
            | AgentTool::Cursor
            | AgentTool::Windsurf
            | AgentTool::Roo => AutomationTier::Automated,
            AgentTool::Copilot
            | AgentTool::Generic
            | AgentTool::Gemini
            | AgentTool::Goose
            | AgentTool::Kiro
            | AgentTool::Qwen
            | AgentTool::Junie
            | AgentTool::Tabnine
            | AgentTool::Trae => AutomationTier::ShimOnly,
        }
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

    /// Path of the file written by `synrepo agent-setup`, relative to the repo root.
    pub(crate) fn output_path(self, repo_root: &Path) -> PathBuf {
        match self {
            // Agent Skills standard: hosts auto-discover `<name>/SKILL.md` under
            // a host-specific or shared `skills/` directory. Codex uses the
            // shared `.agents/skills` repository location.
            AgentTool::Claude => repo_root
                .join(".claude")
                .join("skills")
                .join("synrepo")
                .join("SKILL.md"),
            AgentTool::Cursor => repo_root
                .join(".cursor")
                .join("skills")
                .join("synrepo")
                .join("SKILL.md"),
            AgentTool::Copilot => repo_root.join("synrepo-copilot-instructions.md"),
            AgentTool::Generic => repo_root.join("synrepo-agents.md"),
            AgentTool::Codex => repo_root
                .join(".agents")
                .join("skills")
                .join("synrepo")
                .join("SKILL.md"),
            AgentTool::Windsurf => repo_root
                .join(".windsurf")
                .join("skills")
                .join("synrepo")
                .join("SKILL.md"),
            AgentTool::OpenCode => repo_root.join("AGENTS.md"),
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
            AgentTool::Qwen => repo_root.join(".qwen").join("commands").join("synrepo.md"),
            AgentTool::Junie => repo_root.join(".junie").join("commands").join("synrepo.md"),
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

    /// User-facing noun for the artifact written by `agent-setup`: `"skill"`
    /// for tools that follow the Agent Skills standard (`SKILL.md` under
    /// `.<tool>/skills/synrepo/`) and `"instructions"` otherwise. Derived
    /// from [`output_path`] so the label cannot drift from the filename.
    pub(crate) fn artifact_label(self) -> &'static str {
        // `repo_root` is only used to build the prefix; the filename it
        // contributes is independent of the root, so any path works here.
        let p = self.output_path(Path::new(""));
        if p.file_name().and_then(|n| n.to_str()) == Some("SKILL.md") {
            "skill"
        } else {
            "instructions"
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
    /// Mirrors the paths hard-coded in the matching `setup_*_mcp` functions
    /// in `commands/setup.rs`; the `mcp_config_relative_path_matches_setup`
    /// test in `agent_shims/tests.rs` pins the two sites together.
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
