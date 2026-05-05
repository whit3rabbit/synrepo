---
name: synrepo
description: Use synrepo in repositories with a .synrepo/ directory. Prefer synrepo cards and search before reading source files cold.
---

# synrepo context

synrepo precomputes a structural graph of the codebase from tree-sitter parsing and git history.

## Codex setup

This skill belongs at `.agents/skills/synrepo/SKILL.md`.

For repo-scoped setup, add this to trusted project `.codex/config.toml`:

```toml
[mcp_servers.synrepo]
command = "synrepo"
args = ["mcp", "--repo", "."]
```

Use `codex mcp add synrepo -- synrepo mcp --repo .` only when you want a user-level server in `~/.codex/config.toml`.
For an npm-distributed build, use `codex mcp add synrepo -- npx -y synrepo mcp --repo .` instead.
To add local non-blocking Codex nudges, run `synrepo setup codex --agent-hooks`.

## Agent doctrine

synrepo is a code-context compiler. When `.synrepo/` exists in the repo root, prefer MCP tools (or the CLI fallback) over cold file reads for orientation, codebase questions, file reviews, broad search, change impact, and pre-edit context.

### Default path

The required sequence for codebase questions, reviews, search routing, and edits is orient, find, impact or risks, edit, tests, changed.

1. Start with `synrepo_orient` before reading the repo cold.
2. Use `synrepo_find` or `synrepo_search` to find candidate files and symbols. `synrepo_find` decomposes broad task language into deterministic lexical anchors before returning empty; for broad lexical searches, prefer `output_mode: "compact"` so results are grouped and token-accounted before opening files.
3. Use `tiny` cards to route and `normal` cards to understand. Use `synrepo_minimum_context` once a focal target is known but the surrounding neighborhood risk is unclear, especially for file reviews and codebase questions.
4. Use `synrepo_impact` (or its shorthand `synrepo_risks`) before editing or reviewing risky files, and `synrepo_tests` before claiming done.
5. Use `synrepo_changed` after edits to review changed context and validation commands.
6. Read full source files or request `deep` cards only after bounded cards identify the target or when the card content is insufficient. Full-file reads are an explicit escalation, not the default first step.

Graph-backed structural facts (files, symbols, edges) remain the authoritative source of truth. Overlay commentary, explain docs, and proposed cross-links are advisory, labeled machine-authored, and freshness-sensitive. Treat stale labels as information, not as errors. **Refresh is explicit**: every tool returns what is currently in the overlay. Fresh commentary refresh requires `synrepo mcp --allow-overlay-writes` and `synrepo_refresh_commentary(target)`.

Client-side nudge hooks for Codex and Claude may remind agents to use synrepo before direct grep, read, review, or edit workflows. These hooks are advisory only; the MCP server remains read-first and does not intercept external tool calls.

### Fast-path routing

Use `synrepo_task_route` or `synrepo task-route` when hooks emit `[SYNREPO_CONTEXT_FAST_PATH]`, `[SYNREPO_DETERMINISTIC_EDIT_CANDIDATE] Intent: ...`, or `[SYNREPO_LLM_NOT_REQUIRED]`. Prefer compact search, cards, context packs, or prepared anchored edits before spending LLM tokens. The signals are advisory only; source mutation still requires `synrepo mcp --allow-source-edits` and `synrepo_apply_anchor_edits`.

Graph export is native to synrepo, not skill-owned. When a user asks for a visual graph of the repository, run `synrepo export --format graph-html`. When they ask for machine-readable graph data, run `synrepo export --format graph-json`. These exports are deterministic convenience outputs from the canonical graph DB; they do not require an API key and are not explain input.

### Do not

- Do not open large files first. Start at `tiny` and escalate only when a specific field forces it.
- Do not read a full source file before synrepo routing has identified it; treat a full-file read as an escalation after the bounded card is insufficient.
- Do not treat overlay commentary, explain docs, or proposed cross-links as canonical source truth. They are advisory prose layered on structural cards.
- Do not trigger explain (`--generate-cross-links`, deep commentary refresh) unless the task justifies the cost.
- Do not expect watch or background behavior unless `synrepo watch` is explicitly running.
- Do not mistake client-side hook nudges for MCP interception or enforcement. They are non-blocking reminders.

### Product boundary

- synrepo stores code facts and bounded operational memory. It is not a task tracker, not session memory, and not cross-session agent memory.
- Any handoff or next-action list is a derived recommendation regenerated from repo state. External task systems own assignment, status, and collaboration.
- Freshness is explicit. A stale label is information, not an error; it is not silently refreshed on read.

## MCP tools (primary interface)

- `synrepo_card target=<id> budget=<tiny|normal|deep>` — structured card for a file or symbol
- `synrepo_search query=<text> [output_mode=compact]` — lexical search across indexed files; compact mode groups matches by file and returns output accounting
- `synrepo_task_route task=<description> [path=<path>]` — classify a task into the cheapest safe route and hook signals
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
synrepo task-route "find auth entrypoints"        # advisory route classifier
synrepo search <query>                           # lexical search
synrepo node <target>                            # node metadata as JSON (accepts paths, symbol names, or node IDs)
synrepo graph query "inbound <target>"            # reverse dependencies
synrepo graph query "outbound <target>"           # forward dependencies
synrepo graph stats                              # counts by type
synrepo export --format graph-html               # visual graph export
synrepo export --format graph-json               # machine-readable graph export
synrepo reconcile                                # refresh graph
synrepo links list                               # cross-link candidates
synrepo findings [--freshness <state>]           # findings summary
```
