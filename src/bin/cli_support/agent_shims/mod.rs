//! Static shim content and output paths for `synrepo agent-setup <tool>`.
//!
//! Each shim teaches an agent how to use synrepo. Shims cover the canonical
//! agent doctrine (embedded verbatim from [`doctrine::DOCTRINE_BLOCK`]), the
//! MCP tool reference, and the CLI fallback. Long-form prose and worked
//! examples live in `skill/SKILL.md`; the shim is the minimum kit an agent
//! needs to use synrepo correctly without reading SKILL.md.

use std::path::{Path, PathBuf};

pub(crate) mod doctrine;
mod shims;

#[cfg(test)]
mod tests;

use shims::{
    CLAUDE_SHIM, CODEX_SHIM, COPILOT_SHIM, CURSOR_SHIM, GEMINI_SHIM, GENERIC_SHIM, GOOSE_SHIM,
    JUNIE_SHIM, KIRO_SHIM, OPENCODE_SHIM, QWEN_SHIM, ROO_SHIM, TABNINE_SHIM, TRAE_SHIM,
    WINDSURF_SHIM,
};

/// Agent CLI target for shim generation.
#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub(crate) enum AgentTool {
    /// Claude Code — writes `.claude/synrepo-context.md`
    Claude,
    /// Cursor — writes `.cursor/synrepo.mdc`
    Cursor,
    /// GitHub Copilot — writes `synrepo-copilot-instructions.md`
    Copilot,
    /// Generic AGENTS.md — writes `synrepo-agents.md`
    Generic,
    /// OpenAI Codex CLI — writes `.codex/instructions.md`
    Codex,
    /// Windsurf — writes `.windsurf/rules/synrepo.md`
    Windsurf,
    /// OpenCode — writes `AGENTS.md`
    OpenCode,
    /// Google Gemini CLI — writes `.gemini/commands/synrepo.toml`
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
            AgentTool::Claude => repo_root.join(".claude").join("synrepo-context.md"),
            AgentTool::Cursor => repo_root.join(".cursor").join("synrepo.mdc"),
            AgentTool::Copilot => repo_root.join("synrepo-copilot-instructions.md"),
            AgentTool::Generic => repo_root.join("synrepo-agents.md"),
            AgentTool::Codex => repo_root.join(".codex").join("instructions.md"),
            AgentTool::Windsurf => repo_root.join(".windsurf").join("rules").join("synrepo.md"),
            AgentTool::OpenCode => repo_root.join("AGENTS.md"),
            AgentTool::Gemini => repo_root
                .join(".gemini")
                .join("commands")
                .join("synrepo.toml"),
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
                "Add `@.claude/synrepo-context.md` to your CLAUDE.md to include this context."
            }
            AgentTool::Cursor => {
                "The rule fragment is in `.cursor/synrepo.mdc`. Enable it in your Cursor rules."
            }
            AgentTool::Copilot => {
                "Paste the contents of `synrepo-copilot-instructions.md` into \
                `.github/copilot-instructions.md`."
            }
            AgentTool::Generic => {
                "Paste the contents of `synrepo-agents.md` into your `AGENTS.md` file."
            }
            AgentTool::Codex => {
                "Codex CLI loads `.codex/instructions.md` automatically from the project root."
            }
            AgentTool::Windsurf => {
                "Windsurf loads `.windsurf/rules/synrepo.md` as a project rule automatically."
            }
            AgentTool::OpenCode => "OpenCode loads `AGENTS.md` as a project rule automatically.",
            AgentTool::Gemini => {
                "Write the synrepo config manually: create `.gemini/commands/synrepo.toml`."
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
                "Write the synrepo config manually: create `.roo/commands/synrepo.md`."
            }
            AgentTool::Tabnine => {
                "Write the synrepo config manually: create `.tabnine/agent/commands/synrepo.toml`."
            }
            AgentTool::Trae => {
                "Trae loads `.trae/skills/synrepo/SKILL.md` as a skill automatically."
            }
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
