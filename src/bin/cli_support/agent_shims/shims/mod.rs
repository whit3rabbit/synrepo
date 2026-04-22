//! Static shim content for `synrepo agent-setup <tool>`.
//!
//! Every shim embeds the canonical doctrine block via `doctrine_block!()` so
//! the shared text is byte-identical at compile time. Target-specific copy is
//! limited to invocation conventions (how the agent's host calls MCP tools,
//! where the shim file is written, framing headers).
//!
//! The five shims that install into `.<tool>/skills/synrepo/SKILL.md`
//! (Claude, Cursor, Codex, Windsurf, Gemini) additionally prepend
//! [`skill_frontmatter!()`] so the file is a valid Agent Skills SKILL.md.

mod basic_targets;
mod markdown_targets;
mod shared;
mod skill_targets;

pub(crate) use basic_targets::{
    GEMINI_SHIM, GOOSE_SHIM, JUNIE_SHIM, KIRO_SHIM, QWEN_SHIM, TABNINE_SHIM, TRAE_SHIM,
};
pub(crate) use markdown_targets::{COPILOT_SHIM, GENERIC_SHIM, OPENCODE_SHIM, ROO_SHIM};
pub(crate) use skill_targets::{CLAUDE_SHIM, CODEX_SHIM, CURSOR_SHIM, WINDSURF_SHIM};
