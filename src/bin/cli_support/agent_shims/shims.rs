//! Static shim content for `synrepo agent-setup <tool>`.
//!
//! Every shim embeds the canonical doctrine block via `doctrine_block!()` so
//! the shared text is byte-identical at compile time. Target-specific copy is
//! limited to invocation conventions (how the agent's host calls MCP tools,
//! where the shim file is written, framing headers).
//!
//! The five shims that install into `.<tool>/skills/synrepo/SKILL.md`
//! (Claude, Cursor, Codex, Windsurf, Gemini) additionally prepend
//! [`skill_frontmatter!()`] so the file is a valid Agent Skills SKILL.md —
//! the standard's hosts skip any SKILL.md that lacks `name` + `description`
//! in YAML frontmatter.

use super::doctrine::doctrine_block;

/// YAML frontmatter block required by the Agent Skills standard. Hosts
/// (Claude Code, Codex CLI, Cursor 2.4+, Windsurf, Gemini CLI) scan skill
/// directories at startup and read this block to decide when to lazy-load
/// the skill body. `name` and `description` are both required.
macro_rules! skill_frontmatter {
    () => {
        "---
name: synrepo
description: Use synrepo in repositories with a .synrepo/ directory. Prefer synrepo cards and search before reading source files cold.
---

"
    };
}

pub(crate) const CLAUDE_SHIM: &str = concat!(
    skill_frontmatter!(),
    "\
# synrepo context

",
    doctrine_block!(),
    "
## MCP tools (primary interface)

Use these when the synrepo MCP server is running:

- `synrepo_card target=<id> budget=<tiny|normal|deep>` — structured card for a file or symbol.
- `synrepo_search query=<text>` — lexical search across indexed files.
- `synrepo_overview` — graph counts and mode summary.
- `synrepo_where_to_edit task=<description>` — file suggestions for a plain-language task.
- `synrepo_change_impact target=<id>` — first-pass reverse dependencies for this file or symbol.
- `synrepo_minimum_context target=<id> budget=<...>` — budget-bounded 1-hop neighborhood.
- `synrepo_entrypoints` — entry-point discovery (binaries, CLI commands, HTTP handlers, lib roots).
- `synrepo_findings [node_id=<id>] [kind=<kind>] [freshness=<state>]` — operator-facing cross-link findings.
- `synrepo_recent_activity [kinds=<list>] [limit=<n>]` — bounded synrepo operational events.

Use `synrepo_search` to find node IDs (format: `file_0000000000000042`, `symbol_0000000000000024`) before calling card, impact, or findings tools.

Treat `synrepo_change_impact` as routing help, not exact blast-radius proof. The current impact signal is file-level and approximate.

## CLI fallback (when MCP is not running)

```
synrepo status                                   # health: mode, graph counts, last reconcile
synrepo status --recent                          # bounded operational history
synrepo graph stats                              # node and edge counts as JSON
synrepo search <query>                           # lexical search
synrepo node <id>                                # node metadata as JSON
synrepo graph query \"inbound <node_id>\"          # what depends on this node
synrepo graph query \"outbound <node_id>\"         # what this node depends on
synrepo graph query \"outbound <node_id> defines\" # filtered by edge kind
synrepo reconcile                                # refresh graph against current files
synrepo links list [--tier <tier>]               # active cross-link candidates
synrepo findings [--node <id>] [--freshness <state>] # audit findings
```
"
);

pub(crate) const CURSOR_SHIM: &str = concat!(
    skill_frontmatter!(),
    "\
# synrepo

synrepo is preconfigured with an MCP server registered in `.cursor/mcp.json`.

",
    doctrine_block!(),
    "
## MCP tools (primary)

- `synrepo_card` — structured card for a file or symbol (budget: tiny/normal/deep)
- `synrepo_search` — lexical search
- `synrepo_overview` — graph counts and mode
- `synrepo_where_to_edit` — file suggestions for a plain-language task
- `synrepo_change_impact` — first-pass reverse-dependency impact for a file or symbol
- `synrepo_minimum_context` — budget-bounded 1-hop neighborhood for a focal node
- `synrepo_entrypoints` — entry-point discovery
- `synrepo_findings` — operator-facing cross-link findings
- `synrepo_recent_activity` — bounded synrepo operational events

Treat `synrepo_change_impact` as approximate routing help, not exact blast-radius proof.

## CLI fallback (when MCP is not running)

```
synrepo status                           # health check
synrepo status --recent                  # bounded operational history
synrepo search <query>                   # find symbols or files by name
synrepo node <id>                        # node metadata as JSON
synrepo graph query \"inbound <node_id>\"  # reverse dependencies
synrepo graph query \"outbound <node_id>\" # forward dependencies
synrepo graph stats                      # counts by type
synrepo reconcile                        # refresh graph
synrepo links list                       # active cross-link candidates
synrepo findings                         # findings report
```
"
);

pub(crate) const COPILOT_SHIM: &str = concat!(
    "\
## synrepo (structural graph)

This repo uses synrepo, a context compiler that precomputes a structural graph of the codebase.

",
    doctrine_block!(),
    "
### MCP tools (primary interface)

- `synrepo_card` — structured card for a file or symbol; budgets: tiny/normal/deep
- `synrepo_search` — lexical search by symbol or file name
- `synrepo_overview` — graph counts and repository mode
- `synrepo_where_to_edit` — file suggestions for a plain-language task description
- `synrepo_change_impact` — first-pass reverse dependencies for a given file or symbol
- `synrepo_minimum_context` — budget-bounded 1-hop neighborhood for a focal node
- `synrepo_entrypoints` — entry-point discovery
- `synrepo_findings` — operator-facing cross-link findings
- `synrepo_recent_activity` — bounded synrepo operational events

Node IDs: `file_0000000000000042`, `symbol_0000000000000024`. Use `synrepo_search` to find them before calling card, impact, or findings tools.

Treat impact output as approximate routing help, not exact blast-radius proof.

### CLI fallback (when MCP is not running)

- `synrepo status` — health: mode, graph counts, last reconcile
- `synrepo status --recent` — bounded operational history
- `synrepo search <query>` — lexical search
- `synrepo node <id>` — node metadata as JSON
- `synrepo graph query \"inbound <id>\"` — reverse dependencies
- `synrepo graph query \"outbound <id>\"` — forward dependencies
- `synrepo graph stats` — node and edge counts
- `synrepo reconcile` — refresh the graph
- `synrepo links list` — active cross-link candidates
- `synrepo findings` — overlay findings
"
);

pub(crate) const GENERIC_SHIM: &str = concat!(
    "\
## synrepo

synrepo is a context compiler: it precomputes a structural graph of the codebase from tree-sitter parsing and git history.

",
    doctrine_block!(),
    "
### MCP tools (primary interface)

- `synrepo_card target=<id> budget=<tiny|normal|deep>` — structured card for a file or symbol.
- `synrepo_search query=<text>` — lexical search across indexed files.
- `synrepo_overview` — graph node counts and repository mode.
- `synrepo_where_to_edit task=<description>` — file suggestions for a plain-language task.
- `synrepo_change_impact target=<id>` — first-pass reverse dependencies.
- `synrepo_minimum_context target=<id> budget=<...>` — budget-bounded 1-hop neighborhood.
- `synrepo_entrypoints` — entry-point discovery.
- `synrepo_findings [node_id=<id>] [kind=<kind>] [freshness=<state>]` — operator-facing cross-link findings.
- `synrepo_recent_activity [kinds=<list>] [limit=<n>]` — bounded synrepo operational events.

Node IDs: `file_0000000000000042`, `symbol_0000000000000024`. Use `synrepo_search` to find them.

Treat impact output as approximate routing help, not exact blast-radius proof.

### CLI fallback (when MCP is not running)

```bash
synrepo status                                    # health check
synrepo status --recent                           # bounded operational history
synrepo search <query>                            # find symbols/files by name
synrepo node <id>                                 # node metadata as JSON
synrepo graph query \"inbound <node_id>\"           # reverse dependencies
synrepo graph query \"outbound <node_id>\"          # forward dependencies
synrepo graph query \"outbound <node_id> defines\"  # filtered by edge kind
synrepo graph stats                               # counts by type
synrepo reconcile                                 # refresh graph against current files
synrepo links list                                # active cross-link candidates
synrepo findings [--freshness <state>]            # findings summary
```
"
);

pub(crate) const CODEX_SHIM: &str = concat!(
    skill_frontmatter!(),
    "\
# synrepo context

synrepo precomputes a structural graph of the codebase from tree-sitter parsing and git history.

",
    doctrine_block!(),
    "
## MCP tools (primary interface)

- `synrepo_card target=<id> budget=<tiny|normal|deep>` — structured card for a file or symbol
- `synrepo_search query=<text>` — lexical search across indexed files
- `synrepo_overview` — graph node counts and repository mode
- `synrepo_where_to_edit task=<description>` — file suggestions for a plain-language task
- `synrepo_change_impact target=<id>` — first-pass reverse dependencies
- `synrepo_minimum_context target=<id> budget=<...>` — budget-bounded 1-hop neighborhood
- `synrepo_entrypoints` — entry-point discovery
- `synrepo_findings [node_id=<id>] [kind=<kind>]` — cross-link findings
- `synrepo_recent_activity [kinds=<list>]` — bounded synrepo operational events

Node IDs: `file_0000000000000042`, `symbol_0000000000000024`. Use `synrepo_search` to find them.

## CLI fallback

```bash
synrepo status                                   # health check
synrepo status --recent                          # bounded operational history
synrepo search <query>                           # lexical search
synrepo node <id>                                # node metadata as JSON
synrepo graph query \"inbound <node_id>\"          # reverse dependencies
synrepo graph query \"outbound <node_id>\"         # forward dependencies
synrepo graph stats                              # counts by type
synrepo reconcile                                # refresh graph
synrepo links list                               # cross-link candidates
synrepo findings [--freshness <state>]           # findings summary
```
"
);

pub(crate) const WINDSURF_SHIM: &str = concat!(
    skill_frontmatter!(),
    "\
# synrepo

synrepo is preconfigured with an MCP server registered in `.windsurf/mcp.json`.

",
    doctrine_block!(),
    "
## MCP tools (primary interface)

- `synrepo_card target=<id> budget=<tiny|normal|deep>` — structured card for a file or symbol
- `synrepo_search query=<text>` — lexical search
- `synrepo_overview` — graph counts and mode
- `synrepo_where_to_edit task=<description>` — file suggestions for a task
- `synrepo_change_impact target=<id>` — first-pass reverse-dependency impact
- `synrepo_minimum_context target=<id> budget=<...>` — budget-bounded 1-hop neighborhood
- `synrepo_entrypoints` — entry-point discovery
- `synrepo_findings [node_id=<id>]` — cross-link findings
- `synrepo_recent_activity` — bounded synrepo operational events

Node IDs: `file_0000000000000042`, `symbol_0000000000000024`. Use `synrepo_search` first.

## CLI fallback

```bash
synrepo status                         # health check
synrepo status --recent                # bounded operational history
synrepo search <query>                 # lexical search
synrepo node <id>                      # node metadata as JSON
synrepo graph query \"inbound <id>\"    # reverse dependencies
synrepo graph query \"outbound <id>\"   # forward dependencies
synrepo graph stats                    # node and edge counts
synrepo reconcile                      # refresh graph
synrepo links list                     # cross-link candidates
synrepo findings                       # findings report
```
"
);

pub(crate) const OPENCODE_SHIM: &str = concat!(
    "\
# synrepo context (OpenCode)

synrepo precomputes a structural graph of this codebase from tree-sitter parsing and git history.

",
    doctrine_block!(),
    "
## MCP tools (primary interface)

OpenCode supports these tools when the synrepo MCP server is registered in `opencode.json`:

- `synrepo_card target=<id> budget=<tiny|normal|deep>` — structured card for a file or symbol.
- `synrepo_search query=<text>` — lexical search.
- `synrepo_overview` — graph counts and mode summary.
- `synrepo_where_to_edit task=<description>` — file suggestions for a task.
- `synrepo_change_impact target=<id>` — first-pass reverse dependencies.
- `synrepo_minimum_context target=<id> budget=<...>` — budget-bounded 1-hop neighborhood.
- `synrepo_entrypoints` — entry-point discovery.
- `synrepo_findings [node_id=<id>]` — operator-facing cross-link findings.
- `synrepo_recent_activity` — bounded synrepo operational events.

Use `synrepo_search` to find node IDs (format: `file_0000000000000042`, `symbol_0000000000000024`).

## CLI fallback

```bash
synrepo status                                   # health check
synrepo status --recent                          # bounded operational history
synrepo search <query>                           # lexical search
synrepo node <id>                                # node metadata as JSON
synrepo graph query \"inbound <node_id>\"          # reverse dependencies
synrepo graph query \"outbound <node_id>\"         # forward dependencies
synrepo graph stats                              # node and edge counts
synrepo reconcile                                # refresh graph
synrepo links list                               # cross-link candidates
synrepo findings                                 # findings report
```
"
);

/// Shim content for new targets that don't have automatic MCP registration.
/// These use basic markdown with the synrepo doctrine embedded.
macro_rules! define_basic_shim {
    ($name:ident, $title:expr) => {
        pub(crate) const $name: &str = concat!(
            $title,
            "

synrepo precomputes a structural graph of this codebase from tree-sitter parsing and git history.

",
            doctrine_block!(),
            "

## Using synrepo

- Run `synrepo init` to initialize the graph.
- Use `synrepo status` to check operational health.
- Use `synrepo search <query>` to find symbols and files.
- Use `synrepo node <id>` to inspect node metadata.
- Use `synrepo graph query \"outbound <node_id>\"` to see dependencies.
- Use `synrepo graph query \"inbound <node_id>\"` to see dependents.

For full MCP tool support, register the synrepo MCP server in your client configuration.
"
        );
    };
}

pub(crate) const GEMINI_SHIM: &str = concat!(
    skill_frontmatter!(),
    "\
# synrepo context (Gemini CLI)

synrepo precomputes a structural graph of this codebase from tree-sitter parsing and git history.

",
    doctrine_block!(),
    "
## Using synrepo

- Run `synrepo init` to initialize the graph.
- Use `synrepo status` to check operational health.
- Use `synrepo search <query>` to find symbols and files.
- Use `synrepo node <id>` to inspect node metadata.
- Use `synrepo graph query \"outbound <node_id>\"` to see dependencies.
- Use `synrepo graph query \"inbound <node_id>\"` to see dependents.

For full MCP tool support, register the synrepo MCP server in your client configuration.
"
);

define_basic_shim!(
    GOOSE_SHIM,
    "# synrepo context (Goose)
"
);

define_basic_shim!(
    KIRO_SHIM,
    "# synrepo context (Kiro CLI)
"
);

define_basic_shim!(
    QWEN_SHIM,
    "# synrepo context (Qwen Code)
"
);

define_basic_shim!(
    JUNIE_SHIM,
    "# synrepo context (Junie)
"
);

pub(crate) const ROO_SHIM: &str = concat!(
    "\
# synrepo

synrepo is preconfigured with an MCP server registered in `.roo/mcp.json`.

",
    doctrine_block!(),
    "
## MCP tools (primary interface)

- `synrepo_card target=<id> budget=<tiny|normal|deep>` — structured card for a file or symbol
- `synrepo_search query=<text>` — lexical search
- `synrepo_overview` — graph counts and mode
- `synrepo_where_to_edit task=<description>` — file suggestions for a task
- `synrepo_change_impact target=<id>` — first-pass reverse-dependency impact
- `synrepo_minimum_context target=<id> budget=<...>` — budget-bounded 1-hop neighborhood
- `synrepo_entrypoints` — entry-point discovery
- `synrepo_findings [node_id=<id>]` — cross-link findings
- `synrepo_recent_activity` — bounded synrepo operational events

Node IDs: `file_0000000000000042`, `symbol_0000000000000024`. Use `synrepo_search` first.

## CLI fallback

```bash
synrepo status                         # health check
synrepo status --recent                # bounded operational history
synrepo search <query>                 # lexical search
synrepo node <id>                      # node metadata as JSON
synrepo graph query \"inbound <id>\"    # reverse dependencies
synrepo graph query \"outbound <id>\"   # forward dependencies
synrepo graph stats                    # node and edge counts
synrepo reconcile                      # refresh graph
synrepo links list                     # cross-link candidates
synrepo findings                       # findings report
```
"
);

define_basic_shim!(
    TABNINE_SHIM,
    "# synrepo context (Tabnine CLI)
"
);

define_basic_shim!(
    TRAE_SHIM,
    "# synrepo context (Trae)
"
);
