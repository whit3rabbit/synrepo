//! Static shim content and output paths for `synrepo agent-setup <tool>`.
//!
//! Each shim teaches an agent how to use synrepo. Shims cover the MCP tools
//! (primary interface) and CLI fallback commands. Long-form explanations live in
//! `skill/SKILL.md`.

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
//   3. MCP tools (primary interface)
//   4. CLI commands (fallback when MCP is not running)
//   5. Trust model

const CLAUDE_SHIM: &str = "\
# synrepo context

synrepo is a context compiler: it precomputes a structural graph of the codebase and \
serves it through MCP tools (primary) and CLI commands (fallback).

## When to use synrepo

Check for a `.synrepo/` directory. If it exists, use synrepo BEFORE reading source \
files cold.

## MCP tools (primary interface)

Use these when the synrepo MCP server is running:

- `synrepo_card target=<id> budget=<tiny|normal|deep>` — structured card for a file or \
  symbol. Budget tiers: tiny (~200 tokens), normal (~500 tokens), deep (~2 k tokens).
- `synrepo_search query=<text>` — lexical search across indexed files.
- `synrepo_overview` — graph counts and mode summary.
- `synrepo_where_to_edit task=<description>` — file suggestions for a plain-language task.
- `synrepo_change_impact target=<id>` — what depends on this file or symbol.

Use `synrepo_search` to find node IDs (format: `file_0000000000000042`, \
`symbol_0000000000000024`) before calling `synrepo_card` or `synrepo_change_impact`.

## CLI fallback (when MCP is not running)

```
synrepo status                                   # health: mode, graph counts, last reconcile
synrepo graph stats                              # node and edge counts as JSON
synrepo search <query>                           # lexical search
synrepo node <id>                                # node metadata as JSON
synrepo graph query \"inbound <node_id>\"          # what depends on this node
synrepo graph query \"outbound <node_id>\"         # what this node depends on
synrepo graph query \"outbound <node_id> defines\" # filtered by edge kind
synrepo reconcile                                # refresh graph against current files
```

## Trust model

- Graph content (`source_store: graph`) — parser-observed facts. Treat as ground truth.
- Overlay content (`source_store: overlay`) — machine-authored suggestions. Secondary only.
";

const CURSOR_SHIM: &str = "\
---
description: synrepo structural graph — MCP tools and CLI fallback
globs: [\"**/*\"]
alwaysApply: false
---

# synrepo

synrepo precomputes a structural graph of this codebase. Check for `.synrepo/` before \
reading files cold.

## MCP tools (primary)

- `synrepo_card` — structured card for a file or symbol (budget: tiny/normal/deep)
- `synrepo_search` — lexical search
- `synrepo_overview` — graph counts and mode
- `synrepo_where_to_edit` — file suggestions for a plain-language task
- `synrepo_change_impact` — reverse-dependency impact for a file or symbol

## CLI fallback (when MCP is not running)

```
synrepo status                           # health check
synrepo search <query>                   # find symbols or files by name
synrepo node <id>                        # node metadata as JSON
synrepo graph query \"inbound <node_id>\"  # reverse dependencies
synrepo graph query \"outbound <node_id>\" # forward dependencies
synrepo graph stats                      # counts by type
synrepo reconcile                        # refresh graph
```
";

const COPILOT_SHIM: &str = "\
## synrepo (structural graph)

This repo uses synrepo, a context compiler that precomputes a structural graph of the \
codebase. Check for `.synrepo/` before reading source files cold.

### MCP tools (primary interface)

- `synrepo_card` — structured card for a file or symbol; budgets: tiny/normal/deep
- `synrepo_search` — lexical search by symbol or file name
- `synrepo_overview` — graph counts and repository mode
- `synrepo_where_to_edit` — file suggestions for a plain-language task description
- `synrepo_change_impact` — what depends on a given file or symbol

Node IDs: `file_0000000000000042`, `symbol_0000000000000024`. \
Use `synrepo_search` to find them before calling card or impact tools.

### CLI fallback (when MCP is not running)

- `synrepo status` — health: mode, graph counts, last reconcile
- `synrepo search <query>` — lexical search
- `synrepo node <id>` — node metadata as JSON
- `synrepo graph query \"inbound <id>\"` — reverse dependencies
- `synrepo graph query \"outbound <id>\"` — forward dependencies
- `synrepo graph stats` — node and edge counts
- `synrepo reconcile` — refresh the graph

Graph content is ground truth. Overlay content is machine-authored and secondary.
";

const GENERIC_SHIM: &str = "\
## synrepo

synrepo is a context compiler: it precomputes a structural graph of the codebase from \
tree-sitter parsing and git history. Use it BEFORE reading source files cold when \
`.synrepo/` exists in the repo root.

### MCP tools (primary interface)

When the synrepo MCP server is running, prefer these task-first tools:

- `synrepo_card target=<id> budget=<tiny|normal|deep>` — structured card for a file or \
  symbol. Budget: tiny (~200 tokens), normal (~500 tokens), deep (~2 k tokens).
- `synrepo_search query=<text>` — lexical search across indexed files.
- `synrepo_overview` — graph node counts and repository mode.
- `synrepo_where_to_edit task=<description>` — file suggestions for a plain-language task.
- `synrepo_change_impact target=<id>` — what depends on this file or symbol.

Node IDs: `file_0000000000000042`, `symbol_0000000000000024`. \
Use `synrepo_search` to find them.

### CLI fallback (when MCP is not running)

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

### Trust model

- `source_store: graph` — parser-observed or git-observed facts. Ground truth.
- `source_store: overlay` — machine-authored suggestions. Treat as secondary.
";
