use super::shared::skill_frontmatter;

pub(crate) const CLAUDE_SHIM: &str = concat!(
    skill_frontmatter!(),
    "\
# synrepo context

",
    crate::cli_support::agent_shims::doctrine::doctrine_block!(),
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
synrepo node <target>                             # node metadata as JSON (accepts paths, symbol names, or node IDs)
synrepo graph query \"inbound <target>\"            # what depends on this node
synrepo graph query \"outbound <target>\"           # what this node depends on
synrepo graph query \"outbound <target> defines\"   # filtered by edge kind
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
    crate::cli_support::agent_shims::doctrine::doctrine_block!(),
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
synrepo node <target>                    # node metadata as JSON (accepts paths, symbol names, or node IDs)
synrepo graph query \"inbound <target>\"  # reverse dependencies
synrepo graph query \"outbound <target>\" # forward dependencies
synrepo graph stats                      # counts by type
synrepo reconcile                        # refresh graph
synrepo links list                       # active cross-link candidates
synrepo findings                         # findings report
```
"
);

pub(crate) const CODEX_SHIM: &str = concat!(
    skill_frontmatter!(),
    "\
# synrepo context

synrepo precomputes a structural graph of the codebase from tree-sitter parsing and git history.

## Codex setup

This skill belongs at `.agents/skills/synrepo/SKILL.md`.

Project-scoped setup writes trusted project `.codex/config.toml`. Global Codex MCP registration is not automated by `synrepo setup`; if you configure `~/.codex/config.toml` manually, launch `synrepo mcp` and pass `repo_root` to repo-addressable tools.

For project-scoped manual setup, edit trusted project `.codex/config.toml` directly:

```toml
[mcp_servers.synrepo]
command = \"synrepo\"
args = [\"mcp\", \"--repo\", \".\"]
```

",
    crate::cli_support::agent_shims::doctrine::doctrine_block!(),
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
synrepo node <target>                             # node metadata as JSON (accepts paths, symbol names, or node IDs)
synrepo graph query \"inbound <target>\"            # reverse dependencies
synrepo graph query \"outbound <target>\"           # forward dependencies
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
    crate::cli_support::agent_shims::doctrine::doctrine_block!(),
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
synrepo node <target>                  # node metadata as JSON (accepts paths, symbol names, or node IDs)
synrepo graph query \"inbound <target>\"    # reverse dependencies
synrepo graph query \"outbound <target>\"   # forward dependencies
synrepo graph stats                    # node and edge counts
synrepo reconcile                      # refresh graph
synrepo links list                     # cross-link candidates
synrepo findings                       # findings report
```
"
);
