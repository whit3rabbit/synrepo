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

use shims::{CLAUDE_SHIM, CODEX_SHIM, COPILOT_SHIM, CURSOR_SHIM, GENERIC_SHIM, WINDSURF_SHIM};

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
        }
    }
}
