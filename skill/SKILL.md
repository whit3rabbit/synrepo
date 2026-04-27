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

The required sequence is orient, find, impact or risks, edit, tests, changed.

1. Start with `synrepo_orient` before reading the repo cold.
2. Use `synrepo_find` or `synrepo_search` to find candidate files and symbols.
3. Use `tiny` cards to route and `normal` cards to understand. Use `synrepo_minimum_context` as the bounded neighborhood step when a focal target is known but the surrounding risk is unclear.
4. Use `synrepo_impact` (or its shorthand `synrepo_risks`) before editing.
5. Use `synrepo_tests` before claiming done.
6. Use `synrepo_changed` after edits to review changed context and validation commands.
7. Read full source files or request `deep` cards only after bounded cards identify the target or when the card content is insufficient. Full-file reads are an explicit escalation, not the default first step.

Rule of thumb: `tiny` to find, `normal` to understand, `deep` to write.

## Trust model

- Graph content is primary.
- Overlay content is advisory.
- Materialized advisory explain docs are advisory overlay output, not canonical graph facts.
- If overlay and graph disagree, trust the graph.
- Freshness is explicit. A stale label is information, not an error; it is not silently refreshed on read.

## Do not

- Do not open large files first. Start at `tiny` and escalate only when a specific field forces it.
- Do not treat overlay commentary as canonical. It is advisory prose layered on structural cards.
- Do not trigger explain (`--generate-cross-links`, deep commentary refresh) unless the task justifies the cost.
- Do not expect watch or background behavior unless `synrepo watch` is explicitly running.

## Core tools

- `synrepo_overview()` — first call on an unfamiliar repo
- `synrepo_orient()` — workflow alias for first-call orientation
- `synrepo_find(task, limit?, budget_tokens?)` — workflow alias for finding candidate cards
- `synrepo_explain(target, budget?, budget_tokens?)` — workflow alias for bounded card lookup
- `synrepo_impact(target, budget?, budget_tokens?)` — workflow alias for risk before editing
- `synrepo_risks(target, budget?, budget_tokens?)` — shorthand for `synrepo_impact`
- `synrepo_tests(scope, budget?, budget_tokens?)` — workflow alias for test discovery
- `synrepo_changed()` — workflow alias for changed-context review
- `synrepo_context_pack(goal?, targets?, budget?, budget_tokens?, include_tests?, include_notes?, limit?)` — batch read-only context artifacts into one token-accounted response
- `synrepo_card(target, budget?)` — card for a symbol or file
- `synrepo_search(query, limit?)` — lexical search
- `synrepo_docs_search(query, limit?)` — advisory advisory commentary search
- `synrepo_where_to_edit(task, limit?)` — ranked edit candidates
- `synrepo_change_impact(target)` — first-pass dependents
- `synrepo_change_risk(target)` — composite risk signal
- `synrepo_minimum_context(target, budget?)` — bounded neighborhood step before deep inspection or full-file reads
- `synrepo_entrypoints(scope?, budget?)` — entrypoint discovery
- `synrepo_test_surface(scope)` — test discovery

## Budget protocol

- `tiny`: orientation and routing
- `normal`: interface and neighborhood
- `deep`: exact source and implementation work
- Use `tiny` cards to orient and route.
- Use `normal` cards to understand a neighborhood.
- Use `deep` cards only before writing code, or when exact source or body details matter.
- Use `synrepo_context_pack` when several known files, symbols, directories, tests, or call paths are needed together; it preserves read-only behavior and returns a shared `context_state`.

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
