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
- `synrepo_change_impact target=<id>` — first-pass reverse dependencies for this file or symbol.
- `synrepo_findings [node_id=<id>] [kind=<kind>] [freshness=<state>]` — list operator-facing cross-link findings.

Use `synrepo_search` to find node IDs (format: `file_0000000000000042`, \
`symbol_0000000000000024`) before calling `synrepo_card`, `synrepo_change_impact`, \
or `synrepo_findings`.

Treat `synrepo_change_impact` as routing help, not exact blast-radius proof. The current \
impact signal is file-level and approximate.

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
synrepo links list [--tier <tier>]               # list active cross-link candidates
synrepo findings [--node <id>] [--freshness <state>] # audit findings
```

## Trust model

- Graph content (`source_store: graph`) — parser-observed facts. Treat as the primary \
  source of truth, while current impact hints remain approximate.
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
- `synrepo_change_impact` — first-pass reverse-dependency impact for a file or symbol
- `synrepo_findings` — list operator-facing cross-link findings

Treat `synrepo_change_impact` as approximate routing help, not exact blast-radius proof.

## CLI fallback (when MCP is not running)

```
synrepo status                           # health check
synrepo search <query>                   # find symbols or files by name
synrepo node <id>                        # node metadata as JSON
synrepo graph query \"inbound <node_id>\"  # reverse dependencies
synrepo graph query \"outbound <node_id>\" # forward dependencies
synrepo graph stats                      # counts by type
synrepo reconcile                        # refresh graph
synrepo links list                       # active cross-link candidates
synrepo findings                         # findings report
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
- `synrepo_change_impact` — first-pass reverse dependencies for a given file or symbol
- `synrepo_findings` — list operator-facing cross-link findings

Node IDs: `file_0000000000000042`, `symbol_0000000000000024`. \
Use `synrepo_search` to find them before calling card, impact, or findings tools.

Treat impact output as approximate routing help, not exact blast-radius proof.

### CLI fallback (when MCP is not running)

- `synrepo status` — health: mode, graph counts, last reconcile
- `synrepo search <query>` — lexical search
- `synrepo node <id>` — node metadata as JSON
- `synrepo graph query \"inbound <id>\"` — reverse dependencies
- `synrepo graph query \"outbound <id>\"` — forward dependencies
- `synrepo graph stats` — node and edge counts
- `synrepo_reconcile` — refresh the graph
- `synrepo links list` — list active cross-link candidates
- `synrepo findings` — view overlay findings

Graph content is the primary source of truth, while current impact hints remain \
approximate. Overlay content is machine-authored and secondary.
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
- `synrepo_change_impact target=<id>` — first-pass reverse dependencies for this file or symbol.
- `synrepo_findings [node_id=<id>] [kind=<kind>] [freshness=<state>]` — list operator-facing cross-link findings.

Node IDs: `file_0000000000000042`, `symbol_0000000000000024`. \
Use `synrepo_search` to find them.

Treat impact output as approximate routing help, not exact blast-radius proof.

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
synrepo links list                                # list active candidates
synrepo findings [--freshness <state>]            # findings summary
```

### Trust model

- `source_store: graph` — parser-observed or git-observed facts. Primary source of truth, while current impact hints remain approximate.
- `source_store: overlay` — machine-authored suggestions. Treat as secondary.
";

const CODEX_SHIM: &str = "\
# synrepo context

synrepo is a context compiler: it precomputes a structural graph of the codebase from \
tree-sitter parsing and git history and serves it through MCP tools (primary) and CLI \
commands (fallback).

## When to use synrepo

Check for a `.synrepo/` directory. If it exists, use synrepo BEFORE reading source files cold.

## MCP tools (primary interface)

- `synrepo_card target=<id> budget=<tiny|normal|deep>` — structured card for a file or symbol
- `synrepo_search query=<text>` — lexical search across indexed files
- `synrepo_overview` — graph node counts and repository mode
- `synrepo_where_to_edit task=<description>` — file suggestions for a plain-language task
- `synrepo_change_impact target=<id>` — first-pass reverse dependencies
- `synrepo_findings [node_id=<id>] [kind=<kind>]` — cross-link findings

Node IDs: `file_0000000000000042`, `symbol_0000000000000024`. Use `synrepo_search` to find them.

## CLI fallback

```bash
synrepo status                                   # health check
synrepo search <query>                           # lexical search
synrepo node <id>                                # node metadata as JSON
synrepo graph query \"inbound <node_id>\"          # reverse dependencies
synrepo graph query \"outbound <node_id>\"         # forward dependencies
synrepo graph stats                              # counts by type
synrepo reconcile                                # refresh graph
synrepo links list                               # cross-link candidates
synrepo findings [--freshness <state>]           # findings summary
```

## Trust model

Graph content is parser-observed facts (primary). Overlay content is machine-authored (secondary).
";

const WINDSURF_SHIM: &str = "\
# synrepo

synrepo is a context compiler: it precomputes a structural graph of this codebase. \
Check for `.synrepo/` before reading source files cold.

## MCP tools (primary interface)

When the synrepo MCP server is running:

- `synrepo_card target=<id> budget=<tiny|normal|deep>` — structured card for a file or symbol
- `synrepo_search query=<text>` — lexical search
- `synrepo_overview` — graph counts and mode
- `synrepo_where_to_edit task=<description>` — file suggestions for a task
- `synrepo_change_impact target=<id>` — first-pass reverse-dependency impact
- `synrepo_findings [node_id=<id>]` — cross-link findings

Node IDs: `file_0000000000000042`, `symbol_0000000000000024`. Use `synrepo_search` first.

## CLI fallback

```bash
synrepo status                         # health check
synrepo search <query>                 # lexical search
synrepo node <id>                      # node metadata as JSON
synrepo graph query \"inbound <id>\"    # reverse dependencies
synrepo graph query \"outbound <id>\"   # forward dependencies
synrepo graph stats                    # node and edge counts
synrepo reconcile                      # refresh graph
synrepo links list                     # cross-link candidates
synrepo findings                       # findings report
```

## Trust model

- Graph content: parser-observed facts. Primary source of truth.
- Overlay content: machine-authored suggestions. Secondary.
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_name() {
        assert_eq!(AgentTool::Claude.display_name(), "Claude Code");
        assert_eq!(AgentTool::Cursor.display_name(), "Cursor");
        assert_eq!(AgentTool::Copilot.display_name(), "GitHub Copilot");
        assert_eq!(AgentTool::Generic.display_name(), "generic (AGENTS.md)");
        assert_eq!(AgentTool::Codex.display_name(), "Codex CLI");
        assert_eq!(AgentTool::Windsurf.display_name(), "Windsurf");
    }

    #[test]
    fn test_output_path() {
        let repo_root = std::path::Path::new("/mock/repo");
        assert_eq!(
            AgentTool::Claude.output_path(repo_root),
            repo_root.join(".claude").join("synrepo-context.md")
        );
        assert_eq!(
            AgentTool::Cursor.output_path(repo_root),
            repo_root.join(".cursor").join("synrepo.mdc")
        );
        assert_eq!(
            AgentTool::Copilot.output_path(repo_root),
            repo_root.join("synrepo-copilot-instructions.md")
        );
        assert_eq!(
            AgentTool::Generic.output_path(repo_root),
            repo_root.join("synrepo-agents.md")
        );
        assert_eq!(
            AgentTool::Codex.output_path(repo_root),
            repo_root.join(".codex").join("instructions.md")
        );
        assert_eq!(
            AgentTool::Windsurf.output_path(repo_root),
            repo_root.join(".windsurf").join("rules").join("synrepo.md")
        );
    }

    #[test]
    fn test_include_instruction() {
        assert!(AgentTool::Claude
            .include_instruction()
            .contains("synrepo-context.md"));
        assert!(AgentTool::Cursor
            .include_instruction()
            .contains("synrepo.mdc"));
        assert!(AgentTool::Copilot
            .include_instruction()
            .contains("synrepo-copilot-instructions.md"));
        assert!(AgentTool::Generic
            .include_instruction()
            .contains("synrepo-agents.md"));
        assert!(AgentTool::Codex
            .include_instruction()
            .contains(".codex/instructions.md"));
        assert!(AgentTool::Windsurf
            .include_instruction()
            .contains(".windsurf/rules/synrepo.md"));
    }

    #[test]
    fn test_shim_content() {
        assert!(AgentTool::Claude
            .shim_content()
            .starts_with("# synrepo context"));
        assert!(AgentTool::Cursor
            .shim_content()
            .starts_with("---\ndescription"));
        assert!(AgentTool::Copilot.shim_content().starts_with("## synrepo"));
        assert!(AgentTool::Generic.shim_content().starts_with("## synrepo"));
        assert!(AgentTool::Codex
            .shim_content()
            .starts_with("# synrepo context"));
        assert!(AgentTool::Windsurf
            .shim_content()
            .starts_with("# synrepo\n"));
    }
}
