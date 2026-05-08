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

For questions, reviews, search routing, and edits: orient, ask or find, impact or risks, edit, tests, changed.

1. Start with `synrepo_orient` before reading the repo cold.
2. Use `synrepo_ask` for broad plain-language tasks needing one bounded, cited task-context packet.
3. Use `synrepo_find` for broad tasks, or `synrepo_search` for exact files, symbols, strings, flags, and tool names. For broad lexical searches, prefer `output_mode: "compact"`.
4. Use `tiny` cards to route and `normal` cards to understand. Use `synrepo_minimum_context` once a focal target is known and neighborhood risk is unclear.
5. Use `synrepo_impact` (or `synrepo_risks`) before risky edits or reviews, and `synrepo_tests` before claiming done.
6. Use `synrepo_changed` after edits to review changed context and validation commands.
7. After stale resumes or lost context, call `synrepo_resume_context` before asking the user to repeat repo state.
8. Read full source files or request `deep` cards only after bounded cards identify the target or prove insufficient.

### MCP repository selection

Project-scoped MCP configs launching `synrepo mcp --repo .` have a default repository, so `repo_root` may be omitted. Passing the absolute root is valid and preferred when known.

Global MCP configs that launch `synrepo mcp` serve registered projects by absolute path. In global or defaultless contexts, pass the current workspace's absolute path as `repo_root` to repo-addressable tools. If a tool reports that a repository is not managed by synrepo, ask the user to run `synrepo project add <path>`; do not bypass registry gating.

Graph-backed structural facts are authoritative. Overlay commentary, explain docs, and proposed cross-links are advisory, machine-authored, and freshness-sensitive. Treat stale labels as information. **Refresh is explicit**: tools return current overlay state. Fresh commentary refresh requires `synrepo mcp --allow-overlay-writes` and `synrepo_refresh_commentary(target)`.

Client-side hooks for Codex and Claude may nudge agents before direct grep, read, review, or edit workflows and emit `[SYNREPO_CONTEXT_FAST_PATH]`, `[SYNREPO_DETERMINISTIC_EDIT_CANDIDATE] Intent: ...`, or `[SYNREPO_LLM_NOT_REQUIRED]`. Hooks are advisory; source mutation still requires `synrepo mcp --allow-source-edits` and `synrepo_apply_anchor_edits`.

### Do not

- Do not open large files first. Start at `tiny` and escalate only when a specific field forces it.
- Do not read a full source file before synrepo routing identifies it; full-file reads are explicit escalation.
- Do not treat overlay commentary, explain docs, or proposed cross-links as canonical source truth. They are advisory prose layered on structural cards.
- Do not trigger explain (`--generate-cross-links`, deep commentary refresh) unless the task justifies the cost.
- Do not ask the user to repeat stale repo context until `synrepo_resume_context` has been tried.
- Do not expect watch or background behavior unless `synrepo watch` is explicitly running.
- Do not mistake client-side hook nudges for MCP enforcement.

### Product boundary

- synrepo stores code facts and bounded operational memory. It is not a task tracker, not session memory, and not cross-session agent memory.
- `synrepo_resume_context` is an advisory repo packet regenerated from existing state. It is not prompt logging, chat history, raw tool-output capture, or generic session memory.
- Handoff or next-action lists are derived recommendations regenerated from repo state. External systems own assignment, status, and collaboration.
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
