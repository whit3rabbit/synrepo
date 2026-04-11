//! Static shim content and output paths for `synrepo agent-setup <tool>`.
//!
//! Each shim is a thin integration file that teaches an agent CLI to use synrepo's
//! Phase 1 commands. Shims never embed long-form explanations — those live in
//! `skill/SKILL.md`. The shim checks for `.synrepo/`, documents CLI commands, and
//! notes when Phase 2 MCP tools land.

use std::path::{Path, PathBuf};

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
}

impl AgentTool {
    /// Human-readable name for display.
    pub(crate) fn display_name(self) -> &'static str {
        match self {
            AgentTool::Claude => "Claude Code",
            AgentTool::Cursor => "Cursor",
            AgentTool::Copilot => "GitHub Copilot",
            AgentTool::Generic => "generic (AGENTS.md)",
        }
    }

    /// Path of the file written by `synrepo agent-setup`, relative to the repo root.
    pub(crate) fn output_path(self, repo_root: &Path) -> PathBuf {
        match self {
            AgentTool::Claude => repo_root.join(".claude").join("synrepo-context.md"),
            AgentTool::Cursor => repo_root.join(".cursor").join("synrepo.mdc"),
            AgentTool::Copilot => repo_root.join("synrepo-copilot-instructions.md"),
            AgentTool::Generic => repo_root.join("synrepo-agents.md"),
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
        }
    }

    /// Static shim content for this target.
    pub(crate) fn shim_content(self) -> &'static str {
        match self {
            AgentTool::Claude => CLAUDE_SHIM,
            AgentTool::Cursor => CURSOR_SHIM,
            AgentTool::Copilot => COPILOT_SHIM,
            AgentTool::Generic => GENERIC_SHIM,
        }
    }
}

// ─── Shim content ────────────────────────────────────────────────────────────
//
// Each shim covers:
//   1. What synrepo is (one sentence)
//   2. When to use it (check for .synrepo/)
//   3. Phase 1 CLI commands (available today)
//   4. Phase 2 note (MCP tools coming later)
//   5. Commands quick reference

const CLAUDE_SHIM: &str = "\
# synrepo context

synrepo is a context compiler: it precomputes a structural graph of the codebase and \
serves it through CLI commands (Phase 1) and an MCP server (Phase 2, coming later).

## When to use synrepo

Check for a `.synrepo/` directory. If it exists, use synrepo commands BEFORE reading \
source files cold.

## Phase 1 — CLI commands (available now)

The MCP server is not yet running. Use these CLI commands for structural graph access:

```
synrepo status                                   # health: mode, graph counts, last reconcile
synrepo graph stats                              # node and edge counts as JSON
synrepo search <query>                           # lexical search across indexed files
synrepo node <id>                                # dump a node's metadata as JSON
synrepo graph query \"inbound <node_id>\"          # what depends on this node
synrepo graph query \"outbound <node_id>\"         # what this node depends on
synrepo graph query \"outbound <node_id> defines\" # filtered by edge kind
synrepo reconcile                                # refresh graph against current files
```

Node IDs use display format: `file_0000000000000042`, `symbol_0000000000000024`.
Use `synrepo search <name>` to find IDs by symbol or file name.

## Phase 2 — MCP tools (coming later)

When the MCP server ships, `synrepo_card`, `synrepo_where_to_edit`, \
`synrepo_change_impact`, and other task-first tools replace these CLI calls. \
See `skill/SKILL.md` for the full Phase 2 interface description.

## Trust model

- Graph content (`source_store: graph`) — parser-observed facts. Treat as ground truth.
- Overlay content (`source_store: overlay`) — machine-authored suggestions. Secondary only.
";

const CURSOR_SHIM: &str = "\
---
description: synrepo structural graph commands for Phase 1 (no MCP yet)
globs: [\"**/*\"]
alwaysApply: false
---

# synrepo (Phase 1)

synrepo precomputes a structural graph of this codebase. Check for `.synrepo/` before \
reading files cold.

## Available CLI commands

```
synrepo status                           # health check
synrepo search <query>                   # find symbols or files by name
synrepo node <id>                        # dump node metadata
synrepo graph query \"inbound <node_id>\"  # reverse dependencies
synrepo graph query \"outbound <node_id>\" # forward dependencies
synrepo graph stats                      # counts by type
synrepo reconcile                        # refresh graph
```

Phase 2 MCP tools (`synrepo_card`, `synrepo_where_to_edit`, etc.) are not yet running. \
Use CLI commands for now.
";

const COPILOT_SHIM: &str = "\
## synrepo (structural graph — Phase 1)

This repo uses synrepo, a context compiler that precomputes a structural graph of the \
codebase. Check for `.synrepo/` before reading source files cold.

### Phase 1 CLI commands (MCP not yet running)

Use these shell commands to navigate the graph:

- `synrepo status` — health: mode, graph counts, last reconcile
- `synrepo search <query>` — lexical search by symbol or file name
- `synrepo node <id>` — node metadata as JSON
- `synrepo graph query \"inbound <id>\"` — what depends on this node
- `synrepo graph query \"outbound <id>\"` — what this node depends on
- `synrepo graph stats` — node and edge counts
- `synrepo reconcile` — refresh the graph

Node IDs: `file_0000000000000042`, `symbol_0000000000000024`. \
Use `synrepo search` to find them.

Graph content is ground truth. Overlay content is machine-authored and secondary.
";

const GENERIC_SHIM: &str = "\
## synrepo

synrepo is a context compiler: it precomputes a structural graph of the codebase from \
tree-sitter parsing and git history. Use it BEFORE reading source files cold when \
`.synrepo/` exists in the repo root.

### Phase 1 — CLI commands

The MCP server is not yet running. Use the CLI for structural graph access:

```bash
synrepo status                                    # health check
synrepo search <query>                            # find symbols/files by name
synrepo node <id>                                 # node metadata as JSON
synrepo graph query \"inbound <node_id>\"           # reverse dependencies
synrepo graph query \"outbound <node_id>\"          # forward dependencies
synrepo graph query \"outbound <node_id> defines\"  # filtered by edge kind
synrepo graph stats                               # counts by type
synrepo reconcile                                 # refresh graph against current files
```

Node IDs: `file_0000000000000042`, `symbol_0000000000000024`.

### Trust model

- `source_store: graph` — parser-observed or git-observed facts. Ground truth.
- `source_store: overlay` — machine-authored suggestions. Treat as secondary.

### Phase 2

When the MCP server ships, task-first tools (`synrepo_card`, `synrepo_where_to_edit`, \
`synrepo_change_impact`, etc.) replace these CLI calls. See `skill/SKILL.md` for the \
full planned interface.
";
