---
name: synrepo
description: Use synrepo in repositories with a .synrepo/ directory. Prefer synrepo cards and search before reading source files cold.
---

# synrepo

Use this skill only when the current repository contains a `.synrepo/` directory.

synrepo is a context compiler. It exposes small structural cards and graph tools through MCP so you can orient, route edits, and estimate first-pass impact without opening large files first.

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

1. Start with `synrepo_overview`, `synrepo_search`, `synrepo_where_to_edit`, or `synrepo_entrypoints`.
2. Use `tiny` cards to orient and route.
3. Use `normal` cards to understand a neighborhood.
4. Use `deep` cards only before writing code, or when exact source or body details matter.

Rule of thumb: `tiny` to find, `normal` to understand, `deep` to write.

## Trust model

- Graph content is primary.
- Overlay content is advisory.
- If overlay and graph disagree, trust the graph.
- Freshness is explicit. A stale label is information, not an error; it is not silently refreshed on read.

## Do not

- Do not open large files first. Start at `tiny` and escalate only when a specific field forces it.
- Do not treat overlay commentary as canonical. It is advisory prose layered on structural cards.
- Do not trigger synthesis (`--generate-cross-links`, deep commentary refresh) unless the task justifies the cost.
- Do not expect watch or background behavior unless `synrepo watch` is explicitly running.

## Core tools

- `synrepo_overview()` — first call on an unfamiliar repo
- `synrepo_card(target, budget?)` — card for a symbol or file
- `synrepo_search(query, limit?)` — lexical search
- `synrepo_where_to_edit(task, limit?)` — ranked edit candidates
- `synrepo_change_impact(target)` — first-pass dependents
- `synrepo_change_risk(target)` — composite risk signal
- `synrepo_entrypoints(scope?, budget?)` — entrypoint discovery
- `synrepo_test_surface(scope)` — test discovery

## Budget protocol

- `tiny`: orientation and routing
- `normal`: interface and neighborhood
- `deep`: exact source and implementation work

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