# Search Routing

Use different search tools for different question shapes.

## Orientation

For broad repository questions, start with:

- `synrepo_orient`
- `synrepo_readiness` when you only need operational trust status
- `synrepo_overview` only when you need the full dashboard

Use these to identify modules, readiness, watch/reconcile state, likely card targets, and subsystem boundaries without pulling dashboard-only fields into context.

## Task routing

For plain-language edit or investigation tasks, call:

- `synrepo_ask(ask, scope?, shape?, ground?, budget?)`
- `synrepo_task_route(task, path?)`
- `synrepo_find(task, limit?, budget_tokens?)`
- `synrepo_where_to_edit(task, limit?)`

These tools are best for questions like:

- "Where should I fix auth validation?"
- "Find the likely files for project switching."
- "Where would I add TUI selection handling?"
- "What should I inspect for MCP registration?"

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

## Exact lexical search

For exact symbols, tool names, function names, flags, JSON keys, CLI args, error strings, or file paths, prefer:

- `synrepo_search(query, literal?, limit?, output_mode?, budget_tokens?)`

Use `output_mode: "compact"` for orientation. Set `literal: true` when an exact code string contains regex metacharacters, for example `Error::Other(anyhow`. If a regex-shaped query fails to compile, search retries as an escaped literal and returns `pattern_mode: "literal_fallback"` plus `warnings`. Adaptive compact output may return grouped previews, a minimal miss, or smaller raw rows for tiny result sets; read `output_accounting` before escalating. Use `output_mode: "cards"` when you would otherwise call `synrepo_card` for each matched file.

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

## Convert natural language into code-shaped probes

When task routing is too broad, convert phrases into likely code terms:

- "agent hooks" -> `agent_hooks`
- "MCP registration" -> `registered_tool_names`, `name = "synrepo_`
- "budget parsing" -> `parse_budget`, `budget_tokens`
- "error response" -> `response_has_error`, `render_error`, `error.code`
- "resource reads" -> `read_resource`, `read_resource_blocking`
- "edit gating" -> `allow-source-edits`, `allow-overlay-writes`, `allow_source_edits`, `allow_overlay_writes`, `apply_anchor_edits`
- "project switching" -> `use_project`, `repo_root`, `StateResolver`
- "watch daemon" -> `watch`, `reconcile`, `writer_lock`

Once a subsystem is known, use `path_filter` if available or narrow with path-shaped searches:

- `mcp/`
- `commands/mcp/`
- `agent_shims/`
- `skill/`
- `tests/mcp/`

## Card escalation

After search returns likely targets:

- use `synrepo_card` with `budget: "tiny"` for many candidates
- use `budget: "normal"` for the best 1-3 targets
- use `budget: "deep"` only when validating implementation details or preparing edits
- use `synrepo_context_pack` when several known files, symbols, directories, tests, call paths, entrypoints, public APIs, risk cards, findings, or recent-activity artifacts are needed together
- use `synrepo_ask` first for broad plain-language tasks that need one bounded, cited task-context packet

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
