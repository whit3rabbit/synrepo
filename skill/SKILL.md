---
name: synrepo
description: Use synrepo in repositories with a .synrepo/ directory. Prefer synrepo cards, compact search, and bounded context before reading source files cold.
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
- broad codebase search before opening files
- exact lexical search for symbols, flags, tool names, schema keys, and string literals
- finding where to edit
- first-pass change impact
- entrypoint discovery
- test-surface discovery
- high-level subsystem understanding

Do not use synrepo for:
- tiny files you already need to edit directly
- files already in working context
- simple config or text files with no meaningful symbols
- raw-source patching after a bounded synrepo card has already identified the exact range

## Default path

The required sequence for codebase questions, reviews, search routing, and edits is orient, search, cards, impact or risks, edit, tests, changed.

1. Start with `synrepo_orient` before reading the repo cold.
2. Use the search protocol below to decide between `synrepo_find`, `synrepo_where_to_edit`, and `synrepo_search`.
3. Use `tiny` cards to route and `normal` cards to understand. Use `synrepo_minimum_context` as the bounded neighborhood step when a focal target is known but the surrounding risk is unclear, especially for file reviews and codebase questions.
4. Use `synrepo_impact` or `synrepo_risks` before editing or reviewing risky files.
5. Use `synrepo_tests` before claiming done.
6. Use `synrepo_changed` after edits to review changed context and validation commands.
7. Read full source files or request `deep` cards only after bounded cards identify the target or when the card content is insufficient. Full-file reads are an explicit escalation, not the default first step.

Rule of thumb: `tiny` to find, `normal` to understand, `deep` to write.

## Search protocol

Use different search tools for different question shapes.

### 1. Orientation

For broad repository questions, start with:

- `synrepo_orient`
- `synrepo_overview`
- `synrepo_readiness` when you only need operational trust status

Use these to identify modules, readiness, watch/reconcile state, likely card targets, and subsystem boundaries.

### 2. Task routing

For plain-language edit or investigation tasks, call:

- `synrepo_find(task, limit?, budget_tokens?)`
- `synrepo_where_to_edit(task, limit?)`

These tools are best for questions like:
- “Where should I fix auth validation?”
- “Find the likely files for project switching.”
- “Where would I add TUI selection handling?”
- “What should I inspect for MCP registration?”

Read diagnostic fields when present:
- `query_attempts`
- `fallback_used`
- `miss_reason`
- `recommended_next_queries`
- `recommended_tool`
- `suggested_card_targets`
- `output_accounting`

If `miss_reason` is `no_index_matches`, do not retry the same broad sentence. Switch to exact lexical probes.

If `miss_reason` is `matches_not_in_graph`, use `synrepo_search` or CLI search to inspect raw hits, then call `synrepo_card` only for graph-backed paths.

### 3. Exact lexical search

For exact symbols, tool names, function names, flags, JSON keys, CLI args, error strings, or file paths, prefer:

- `synrepo_search(query, limit?, output_mode?, budget_tokens?)`

Use `output_mode: "compact"` for orientation. Use `output_mode: "cards"` when you would otherwise call `synrepo_card` for each matched file.

Prefer exact probes over broad natural language when the task includes identifiers or likely identifiers.

Examples:
- `parse_budget`
- `response_has_error`
- `read_resource_blocking`
- `allow-source-edits`
- `allow-overlay-writes`
- `synrepo_refresh_commentary`
- `synrepo_note_add`
- `registered_tool_names`
- `name = "synrepo_`
- `budget_tokens`
- `repo_root`
- `RATE_LIMITED`
- `BUSY`

Do not use a full sentence when an exact token or string literal is known.

### 4. Convert natural language into code-shaped probes

When task routing is too broad, convert phrases into likely code terms:

- “agent hooks” → `agent_hooks`
- “MCP registration” → `registered_tool_names`, `name = "synrepo_`
- “budget parsing” → `parse_budget`, `budget_tokens`
- “error response” → `response_has_error`, `render_error`, `error.code`
- “resource reads” → `read_resource`, `read_resource_blocking`
- “edit gating” → `allow-source-edits`, `allow-overlay-writes`, `allow_source_edits`, `allow_overlay_writes`, `apply_anchor_edits`
- “project switching” → `use_project`, `repo_root`, `StateResolver`
- “watch daemon” → `watch`, `reconcile`, `writer_lock`

Once a subsystem is known, use `path_filter` if available or narrow with path-shaped searches:
- `mcp/`
- `commands/mcp/`
- `agent_shims/`
- `skill/`
- `tests/mcp/`

### 5. Card escalation

After search returns likely targets:

- use `synrepo_card` with `budget: "tiny"` for many candidates
- use `budget: "normal"` for the best 1-3 targets
- use `budget: "deep"` only when validating implementation details or preparing edits
- use `synrepo_context_pack` when several known files, symbols, directories, tests, or call paths are needed together

### 6. CLI fallback

Use `st`, `rg`, direct file reads, or normal repository tools when:

- MCP tools are unavailable
- `synrepo_search` returns zero results for exact tokens that should exist
- the graph is stale or compatibility requires rebuild
- the code path is not represented in graph-backed cards
- raw source ranges are required for patching
- tests, formatting, linting, or build commands must be run

Do not treat CLI fallback as failure. Treat it as raw-source verification after bounded synrepo routing.

## Search failure handling

When a synrepo route misses or returns weak results:

1. Inspect `query_attempts`, `fallback_used`, and `miss_reason` when present.
2. If `recommended_next_queries` is non-empty, retry those exact probes with `recommended_tool`, usually `synrepo_search`.
3. If all attempts are broad phrases, retry with exact identifiers or string literals.
4. If the task names a flag, tool, schema key, function, module, or file, use `synrepo_search` before `synrepo_find`.
5. If compact search finds candidate files, escalate to `synrepo_card`.
6. If cards are stale, incomplete, or not graph-backed, verify with raw source search.
7. If the repo state is stale, run status/check/reconcile/sync through the CLI when allowed.
8. Never claim an issue is confirmed from a broad finder result alone. Confirm against cards or source.

## MCP edit path

When the server was explicitly started with `synrepo mcp --allow-source-edits`, prefer read tools first, then call `synrepo_prepare_edit_context` before `synrepo_apply_anchor_edits`.

Without that process flag, source-edit tools are absent. Overlay write tools such as notes and commentary refresh are also absent unless the server was started with `synrepo mcp --allow-overlay-writes`. Config may further restrict mutation, but config alone does not enable mutation tools.

Do not call `synrepo_apply_anchor_edits` without a fresh `synrepo_prepare_edit_context` response for the target range. Prepared anchors are session-scoped operational state.

## Project-scoped and global MCP

Project-scoped MCP configs that launch `synrepo mcp --repo .` have a default repository, so `repo_root` may be omitted. Passing the absolute repository root is still valid and preferred when you can identify it reliably.

Global MCP configs that launch `synrepo mcp` serve registered projects by absolute path. In global or defaultless contexts, pass the current workspace's absolute path as `repo_root` to repo-addressable tools, or call `synrepo_use_project(repo_root)` once to set the session default.

Resource-aware MCP hosts may also address managed projects explicitly with `synrepo://project/{project_id}/card/{target}`, `synrepo://project/{project_id}/file/{path}/outline`, or `synrepo://project/{project_id}/context-pack?goal={goal}`. Use `synrepo://projects` to list stable project IDs.

If a tool reports that a repository is not managed by synrepo, ask the user to run:

```bash
synrepo project add <path>
````

Do not bypass registry gating.

## Fast-path routing

Use `synrepo_task_route(task, path?)` or `synrepo task-route <task> [--path <path>]` when you want the cheapest safe route before reading files or asking an LLM to transform code. It returns `intent`, `confidence`, `recommended_tools`, `budget_tier`, `llm_required`, optional `edit_candidate`, stable `signals`, and a short `reason`.

Hook signals mean:

Client-side nudge hooks for Codex and Claude may remind agents to use synrepo before direct grep, read, review, or edit workflows.

* `[SYNREPO_CONTEXT_FAST_PATH]`: prefer compact search, cards, or context packs before cold file reads.
* `[SYNREPO_DETERMINISTIC_EDIT_CANDIDATE] Intent: <intent>`: a narrow mechanical edit may be possible, but prepare anchors first.
* `[SYNREPO_LLM_NOT_REQUIRED]`: structural context or anchored edits should be enough for the next step.

V1 edit candidates are advisory only: `var-to-const`, `remove-debug-logging`, `replace-literal`, and `rename-local`. Reject `add-types`, `add-error-handling`, `async-await`, and broad rename as LLM-required unless the user supplies a prepared anchor range. Hooks never apply edits.

## Trust model

* Graph content is primary.
* Overlay content is advisory.
* Materialized advisory explain docs are overlay output, not canonical graph facts.
* Prepared edit anchors are session-scoped operational state. They are not graph facts, overlay content, commentary, agent notes, or agent memory.
* If overlay and graph disagree, trust the graph.
* Freshness is explicit. A stale label is information, not an error; it is not silently refreshed on read.

## Do not

* Do not open large files first. Start at `tiny` and escalate only when a specific field forces it.
* Do not treat overlay commentary, explain docs, or proposed cross-links as canonical source truth.
* Do not trigger explain (`--generate-cross-links`, deep commentary refresh) unless the task justifies the cost.
* Do not expect watch or background behavior unless `synrepo watch` is explicitly running.
* Do not call `synrepo_apply_anchor_edits` without a fresh `synrepo_prepare_edit_context` response.
* Do not expect synrepo MCP edit tools to run shell commands. Command execution is unavailable.
* Do not mistake client-side hook nudges for MCP interception or enforcement. They are non-blocking reminders.
* Do not retry the same failed broad `synrepo_find` query repeatedly. Convert it to exact `synrepo_search` probes.
* Do not claim validation from search hits alone. Confirm with cards, source, or tests.

## Core tools

* `synrepo_overview()` — first call on an unfamiliar repo.
* `synrepo_readiness()` — cheap read-only preflight for graph, overlay, index, watch, reconcile, and enabled MCP mutation modes.
* `synrepo_orient()` — workflow alias for first-call orientation.
* `synrepo_find(task, limit?, budget_tokens?)` — task-oriented routing for plain-language questions. Best for “where should I look?” Not the best first tool for exact symbols, string literals, flags, schema fields, tool names, or file paths.
* `synrepo_where_to_edit(task, limit?)` — ranked edit candidates for plain-language edit tasks. Inspect diagnostics and switch to exact search when broad routing misses.
* `synrepo_search(query, limit?, output_mode?, budget_tokens?)` — lexical search. Best for exact symbols, string literals, CLI flags, MCP tool names, schema keys, file paths, and code-review validation. Use `output_mode: "compact"` to group matches by file and return `output_accounting`; use `output_mode: "cards"` to return tiny file cards directly.
* `synrepo_explain(target, budget?, budget_tokens?)` — workflow alias for bounded card lookup.
* `synrepo_card(target?, targets?, budget?, budget_tokens?)` — card for one symbol/file, or up to 10 cards in one batch.
* `synrepo_minimum_context(target, budget?)` — bounded neighborhood step before deep inspection or full-file reads.
* `synrepo_context_pack(goal?, targets?, budget?, budget_tokens?, output_mode?, include_tests?, include_notes?, limit?)` — batch read-only context artifacts into one token-accounted response; compact mode applies to search artifacts.
* `synrepo_impact(target, budget?, budget_tokens?)` — workflow alias for risk before editing.
* `synrepo_risks(target, budget?, budget_tokens?)` — shorthand for `synrepo_impact`.
* `synrepo_change_impact(target, direction?)` — first-pass dependents and optional outbound dependencies.
* `synrepo_change_risk(target)` — composite risk signal.
* `synrepo_tests(scope, budget?, budget_tokens?)` — workflow alias for test discovery.
* `synrepo_test_surface(scope)` — test discovery.
* `synrepo_changed()` — workflow alias for changed-context review.
* `synrepo_task_route(task, path?)` — classify a task into the cheapest safe route and stable hook signals.
* `synrepo_docs_search(query, limit?)` — advisory commentary search.
* `synrepo_notes(include_hidden?)` — read advisory overlay notes.
* `synrepo_refactor_suggestions(min_lines?, limit?, path_filter?)` — large non-test source files with modularity hints.
* `synrepo_entrypoints(scope?, budget?)` — entrypoint discovery.
* `synrepo_metrics()` — this-session and persisted MCP/context metrics.
* `synrepo_use_project(repo_root)` — set the default repo for a global/defaultless MCP session.

Overlay-write tools, present only under `synrepo mcp --allow-overlay-writes`:

* `synrepo_refresh_commentary(scope, target?)` — explicitly generate or refresh advisory commentary.
* `synrepo_note_add(...)`, `synrepo_note_link(...)`, `synrepo_note_supersede(...)`, `synrepo_note_forget(...)`, `synrepo_note_verify(...)` — mutate advisory overlay notes.

Source-edit tools, present only under `synrepo mcp --allow-source-edits`:

* `synrepo_prepare_edit_context(target, target_kind?, start_line?, end_line?, task_id?, budget_lines?)` — prepare session-scoped line anchors and compact source context.
* `synrepo_apply_anchor_edits(edits, diagnostics_budget?)` — validate prepared anchors and apply atomic cross-file edits.

## Budget protocol

* `tiny`: orientation and routing
* `normal`: interface and neighborhood
* `deep`: exact source and implementation work

Use `tiny` cards to orient and route.
Use `normal` cards to understand a neighborhood.
Use `deep` cards only before writing code, or when exact source or body details matter.

Use `synrepo_context_pack` when several known files, symbols, directories, tests, or call paths are needed together; it preserves read-only behavior and returns a shared `context_state`.

Unknown budget strings are invalid parameters. Valid tiers are exactly `tiny`, `normal`, and `deep`.

Cards are synrepo's native compact context format. Use compact search to route, then cards or context packs for bounded detail, then full source only when the bounded context is insufficient.

MCP errors are structured. Branch on `error.code` when present and use `error_message` only as a compatibility fallback.

Read/card tools are rate-limited. If you receive `RATE_LIMITED`, wait briefly or reduce batching. If you receive `BUSY`, retry after the current read pressure clears.

Default to `tiny`.

## Fallback

If MCP is unavailable, use the CLI:

```bash
synrepo status
synrepo check
synrepo task-route "find auth entrypoints"
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
