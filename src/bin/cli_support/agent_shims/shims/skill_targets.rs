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
- `synrepo_search query=<text> [output_mode=compact]` — lexical search across indexed files.
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

synrepo is a local, deterministic code-context compiler. It compiles repository files into observed graph facts, code artifacts, task contexts, and cards/MCP packets before agents read source cold.

## Codex setup

This skill belongs at `.agents/skills/synrepo/SKILL.md`.

Project-scoped setup writes trusted project `.codex/config.toml`. Global Codex MCP registration is not automated by `synrepo setup`; if you configure `~/.codex/config.toml` manually, launch `synrepo mcp` and pass `repo_root` to repo-addressable tools. To add local non-blocking Codex nudges, run `synrepo setup codex --agent-hooks`.

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

- `synrepo_readiness()` - cheap read-only preflight for graph, overlay, index, watch, reconcile, and edit-mode status
- `synrepo_orient()` - workflow step 1: small routing summary before reading the repo cold
- `synrepo_ask(ask, scope?, shape?, ground?, budget?)` - default high-level front door for one bounded, cited task-context packet
- `synrepo_search(query, literal?, limit?, output_mode?, budget_tokens?)` - exact lexical search for symbols, flags, code strings, schema keys, file paths, and validation
- `synrepo_explain(target, budget?)` - bounded card lookup; use `budget=deep` for 1-3 focal targets when existing overlay commentary would help
- `synrepo_docs_search(query, limit?)` - advisory search over existing materialized explain docs for architecture, intent, gotchas, and why questions
- `synrepo_card(target?, targets?, budget?, budget_tokens?)` - structured card for one file or symbol, or a small batch
- `synrepo_context_pack(goal?, targets?, budget?, budget_tokens?, output_mode?, include_tests?, include_notes?, limit?)` - batch known read-only code artifacts and task-context pieces into one token-accounted response
- `synrepo_task_route(task, path?)` - cheap route classification when only intent, budget, and next tools are needed
- `synrepo_minimum_context(target, budget?)` - bounded neighborhood once a focal target is known
- `synrepo_impact(target)` or `synrepo_risks(target)` - first-pass change-risk context before edits or risky reviews
- `synrepo_tests(scope)` - discover likely validation commands before claiming done
- `synrepo_changed()` - review changed context and validation guidance after edits
- `synrepo_resume_context(limit?, since_days?, budget_tokens?, include_notes?)` - compact repo resume packet before asking the user to repeat stale context
- `synrepo_overview()` - full dashboard only when the full operational picture is useful

`synrepo_ask` returns `answer`, `cards_used`, `evidence`, `grounding`, `omitted_context_notes`, `next_best_tools`, and `context_packet`. Its grounding policy accepts `mode` or `citations`, `include_spans`, and `allow_overlay`; default to observed graph/index evidence and allow overlay only when advisory machine-authored context is acceptable.

Graph facts are authoritative observed source truth. Overlay commentary, explain docs, and notes are advisory; LLM-authored output never mutates the canonical graph. Embeddings are optional routing/search helpers and are not the core trust source.

Node IDs: `file_0000000000000042`, `symbol_0000000000000024`. Use `synrepo_search` to find them.

## CLI fallback

```bash
synrepo status                                   # health check
synrepo status --recent                          # bounded operational history
synrepo resume-context --json                    # compact repo resume packet
synrepo task-route \"find auth entrypoints\"        # advisory route classifier
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
