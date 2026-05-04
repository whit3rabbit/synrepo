---
name: synrepo
description: Use synrepo in repositories with a .synrepo/ directory. Prefer synrepo cards and search before reading source files cold.
---

# synrepo

Use this skill only when the current repository contains a `.synrepo/` directory.

For product overview, setup flow, and operator-facing docs, start with [`README.md`](../README.md).
This file is the agent operating guide: how to query synrepo safely once the repo is already wired.

## Use when

Use synrepo for:
- orienting on an unfamiliar repo
- answering codebase questions
- reviewing files or subsystems
- broad lexical search before opening files
- finding where to edit
- first-pass change impact
- entrypoint discovery
- test-surface discovery
- high-level subsystem understanding

Do not use synrepo for:
- tiny files you already need to edit directly
- files already in working context
- simple config or text files with no meaningful symbols

## Default path

The required sequence for codebase questions, reviews, search routing, and edits is orient, find, impact or risks, edit, tests, changed.

1. Start with `synrepo_orient` before reading the repo cold.
2. Use `synrepo_find` or `synrepo_search` to find candidate files and symbols. For broad lexical searches, prefer `output_mode: "compact"` so results are grouped and token-accounted before opening files.
3. Use `tiny` cards to route and `normal` cards to understand. Use `synrepo_minimum_context` as the bounded neighborhood step when a focal target is known but the surrounding risk is unclear, especially for file reviews and codebase questions.
4. Use `synrepo_impact` (or its shorthand `synrepo_risks`) before editing or reviewing risky files.
5. Use `synrepo_tests` before claiming done.
6. Use `synrepo_changed` after edits to review changed context and validation commands.
7. Read full source files or request `deep` cards only after bounded cards identify the target or when the card content is insufficient. Full-file reads are an explicit escalation, not the default first step.

When the server was explicitly started with `synrepo mcp --allow-edits`, prefer read tools first, then call `synrepo_prepare_edit_context` before `synrepo_apply_anchor_edits`. Without that process flag, edit tools are absent. Config may further restrict editing, but config alone does not enable mutation tools.

Project-scoped MCP configs that launch `synrepo mcp --repo .` have a default repository, so `repo_root` may be omitted. Passing the absolute repository root is still valid and preferred when you can identify it reliably.

Global MCP configs that launch `synrepo mcp` serve registered projects by absolute path. In global or defaultless contexts, pass the current workspace's absolute path as `repo_root` to repo-addressable tools. If a tool reports that a repository is not managed by synrepo, ask the user to run `synrepo project add <path>`; do not bypass registry gating.

Rule of thumb: `tiny` to find, `normal` to understand, `deep` to write.

Client-side nudge hooks for Codex and Claude may remind agents to use synrepo before direct grep, read, review, or edit workflows. Install them with `synrepo setup codex --agent-hooks` or `synrepo setup claude --agent-hooks`. These hooks are advisory reminders. They do not block tools, store prompt content, or make the MCP server intercept external tool calls.

## Trust model

- Graph content is primary.
- Overlay content is advisory.
- Materialized advisory explain docs are advisory overlay output, not canonical graph facts.
- Prepared edit anchors are session-scoped operational state. They are not graph facts, overlay content, commentary, agent notes, or agent memory.
- If overlay and graph disagree, trust the graph.
- Freshness is explicit. A stale label is information, not an error; it is not silently refreshed on read.

## Do not

- Do not open large files first. Start at `tiny` and escalate only when a specific field forces it.
- Do not treat overlay commentary as canonical. It is advisory prose layered on structural cards.
- Do not trigger explain (`--generate-cross-links`, deep commentary refresh) unless the task justifies the cost.
- Do not expect watch or background behavior unless `synrepo watch` is explicitly running.
- Do not call `synrepo_apply_anchor_edits` without a fresh `synrepo_prepare_edit_context` response.
- Do not expect synrepo MCP edit tools to run shell commands. Command execution is unavailable.
- Do not mistake client-side hook nudges for MCP interception or enforcement. They are non-blocking reminders.

## Core tools

- `synrepo_overview()` — first call on an unfamiliar repo
- `synrepo_orient()` — workflow alias for first-call orientation
- `synrepo_find(task, limit?, budget_tokens?)` — workflow alias for finding candidate cards
- `synrepo_explain(target, budget?, budget_tokens?)` — workflow alias for bounded card lookup
- `synrepo_impact(target, budget?, budget_tokens?)` — workflow alias for risk before editing
- `synrepo_risks(target, budget?, budget_tokens?)` — shorthand for `synrepo_impact`
- `synrepo_tests(scope, budget?, budget_tokens?)` — workflow alias for test discovery
- `synrepo_changed()` — workflow alias for changed-context review
- `synrepo_context_pack(goal?, targets?, budget?, budget_tokens?, output_mode?, include_tests?, include_notes?, limit?)` — batch read-only context artifacts into one token-accounted response; compact mode applies to search artifacts
- `synrepo_card(target, budget?)` — card for a symbol or file
- `synrepo_search(query, limit?, output_mode?, budget_tokens?)` — lexical search; `output_mode: "compact"` groups matches by file and returns `output_accounting`
- `synrepo_docs_search(query, limit?)` — advisory advisory commentary search
- `synrepo_where_to_edit(task, limit?)` — ranked edit candidates
- `synrepo_change_impact(target)` — first-pass dependents
- `synrepo_change_risk(target)` — composite risk signal
- `synrepo_minimum_context(target, budget?)` — bounded neighborhood step before deep inspection or full-file reads
- `synrepo_entrypoints(scope?, budget?)` — entrypoint discovery
- `synrepo_test_surface(scope)` — test discovery

Edit-enabled tools, present only under `synrepo mcp --allow-edits`:
- `synrepo_prepare_edit_context(target, target_kind?, start_line?, end_line?, task_id?, budget_lines?)` — prepare session-scoped line anchors and compact source context
- `synrepo_apply_anchor_edits(edits, diagnostics_budget?)` — validate prepared anchors and apply atomic per-file edits

## Budget protocol

- `tiny`: orientation and routing
- `normal`: interface and neighborhood
- `deep`: exact source and implementation work
- Use `tiny` cards to orient and route.
- Use `normal` cards to understand a neighborhood.
- Use `deep` cards only before writing code, or when exact source or body details matter.
- Use `synrepo_context_pack` when several known files, symbols, directories, tests, or call paths are needed together; it preserves read-only behavior and returns a shared `context_state`.
- Cards are synrepo's native compact context format. Use compact search to route, then cards or context packs for bounded detail, then full source only when the bounded context is insufficient.

Default to `tiny`.

## Fallback

If MCP is unavailable, use the CLI:

```bash
synrepo status
synrepo check
synrepo search "term"
synrepo graph stats
synrepo reconcile
synrepo sync
```

If neither MCP nor the CLI is available, fall back to normal file reading.

## Product boundary

synrepo stores code facts and bounded operational memory. It is not a task tracker, not session memory, and not cross-session agent memory.
Any handoff or next-action list is a derived recommendation regenerated from repo state. External task systems own assignment, status, and collaboration.
Prepared edit anchors are short-lived MCP operational state, not durable agent memory.
