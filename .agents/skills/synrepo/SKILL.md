---
name: synrepo
description: Use synrepo in repositories with a .synrepo/ directory. Prefer synrepo cards, compact search, and bounded task contexts before reading source files cold.
---

# synrepo context

synrepo is a local, deterministic code-context compiler. It compiles repository files into observed graph facts, code artifacts, task contexts, and cards/MCP packets before agents read source cold.

## Codex setup

This skill belongs at `.agents/skills/synrepo/SKILL.md`.

Project-scoped setup writes trusted project `.codex/config.toml`. Global Codex MCP registration is not automated by `synrepo setup`; if you configure `~/.codex/config.toml` manually, launch `synrepo mcp` and pass `repo_root` to repo-addressable tools. To add local non-blocking Codex nudges, run `synrepo setup codex --agent-hooks`.

For project-scoped manual setup, edit trusted project `.codex/config.toml` directly:

```toml
[mcp_servers.synrepo]
command = "synrepo"
args = ["mcp", "--repo", "."]
```

## Agent doctrine

synrepo is a local, deterministic code-context compiler: `repo files -> graph facts -> code artifacts -> task contexts -> cards/MCP`. In `.synrepo/` repos, prefer MCP/CLI over cold reads for questions, reviews, search, impact, and edits.

### Default path

The required sequence for questions, reviews, search routing, and edits is orient, ask or find, impact or risks, edit, tests, changed.

1. Start with `synrepo_orient` before reading the repo cold.
2. Use `synrepo_ask` for broad plain-language tasks that need one bounded, cited task-context packet.
3. Use `synrepo_find` or `synrepo_search` to drill down to files and symbols. `synrepo_find` decomposes broad language into deterministic lexical anchors before returning empty; for broad lexical searches, prefer `output_mode: "compact"`.
4. Use `tiny` cards to route and `normal` cards to understand. Use `synrepo_minimum_context` once a focal target is known but the surrounding neighborhood risk is unclear, especially for file reviews and codebase questions.
5. Use `synrepo_impact` (or its shorthand `synrepo_risks`) before editing or reviewing risky files, and `synrepo_tests` before claiming done.
6. Use `synrepo_changed` after edits to review changed context and validation commands.
7. Read full source files or request `deep` cards only after bounded cards identify the target or when the card content is insufficient. Full-file reads are an explicit escalation, not the default first step.

### MCP repository selection

Project-scoped MCP configs that launch `synrepo mcp --repo .` have a default repository, so `repo_root` may be omitted. Passing the absolute repository root is still valid and preferred when you can identify it reliably.

Global MCP configs that launch `synrepo mcp` serve registered projects by absolute path. In global or defaultless contexts, pass the current workspace's absolute path as `repo_root` to repo-addressable tools. If a tool reports that a repository is not managed by synrepo, ask the user to run `synrepo project add <path>`; do not bypass registry gating.

Graph-backed structural facts (files, symbols, edges) remain the authoritative source of truth. Overlay commentary, explain docs, and proposed cross-links are advisory, labeled machine-authored, and freshness-sensitive. Treat stale labels as information, not as errors. **Refresh is explicit**: every tool returns what is currently in the overlay. Fresh commentary refresh requires `synrepo mcp --allow-overlay-writes` and `synrepo_refresh_commentary(target)`.

Client-side nudge hooks for Codex and Claude may remind agents to use synrepo before direct grep, read, review, or edit workflows and emit `[SYNREPO_CONTEXT_FAST_PATH]`, `[SYNREPO_DETERMINISTIC_EDIT_CANDIDATE] Intent: ...`, or `[SYNREPO_LLM_NOT_REQUIRED]`. Hooks are advisory only; source mutation still requires `synrepo mcp --allow-source-edits` and `synrepo_apply_anchor_edits`.

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

- `synrepo_readiness()` - cheap read-only preflight for graph, overlay, index, watch, reconcile, and edit-mode status
- `synrepo_orient()` - workflow step 1: small routing summary before reading the repo cold
- `synrepo_ask(ask, scope?, shape?, ground?, budget?)` - default high-level front door for one bounded, cited task-context packet
- `synrepo_search(query, limit?, output_mode?, budget_tokens?)` - exact lexical search for symbols, flags, schema keys, file paths, and validation
- `synrepo_card(target?, targets?, budget?, budget_tokens?)` - structured card for one file or symbol, or a small batch
- `synrepo_context_pack(goal?, targets?, budget?, budget_tokens?, output_mode?, include_tests?, include_notes?, limit?)` - batch known read-only code artifacts and task-context pieces into one token-accounted response
- `synrepo_task_route(task, path?)` - cheap route classification when only intent, budget, and next tools are needed
- `synrepo_minimum_context(target, budget?)` - bounded neighborhood once a focal target is known
- `synrepo_impact(target)` or `synrepo_risks(target)` - first-pass change-risk context before edits or risky reviews
- `synrepo_tests(scope)` - discover likely validation commands before claiming done
- `synrepo_changed()` - review changed context and validation guidance after edits
- `synrepo_overview()` - full dashboard only when the full operational picture is useful

`synrepo_ask` returns `answer`, `cards_used`, `evidence`, `grounding`, `omitted_context_notes`, `next_best_tools`, and `context_packet`. Its grounding policy accepts `mode` or `citations`, `include_spans`, and `allow_overlay`; default to observed graph/index evidence and allow overlay only when advisory machine-authored context is acceptable.

Graph facts are authoritative observed source truth. Overlay commentary, explain docs, and notes are advisory; LLM-authored output never mutates the canonical graph. Embeddings are optional routing/search helpers and are not the core trust source.

Node IDs: `file_0000000000000042`, `symbol_0000000000000024`. Use `synrepo_search` to find them.

## CLI fallback

```bash
synrepo status                                   # health check
synrepo status --recent                          # bounded operational history
synrepo task-route "find auth entrypoints"        # advisory route classifier
synrepo search <query>                           # lexical search
synrepo node <target>                             # node metadata as JSON (accepts paths, symbol names, or node IDs)
synrepo graph query "inbound <target>"            # reverse dependencies
synrepo graph query "outbound <target>"           # forward dependencies
synrepo graph stats                              # counts by type
synrepo reconcile                                # refresh graph
synrepo links list                               # cross-link candidates
synrepo findings [--freshness <state>]           # findings summary
```
