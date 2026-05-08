# Budgets And Errors

## Budget protocol

- `tiny`: orientation and routing
- `normal`: interface and neighborhood
- `deep`: exact source and implementation work

Use `tiny` cards to orient and route.
Use `normal` cards to understand a neighborhood.
Use `deep` cards only before writing code, or when exact source or body details matter.

Use `synrepo_context_pack` when several known files, symbols, directories, tests, call paths, entrypoints, public APIs, risk cards, findings, recent activity, or other task-context pieces are needed together; it preserves read-only behavior and returns a shared `context_state`.

Unknown budget strings are invalid parameters. Valid tiers are exactly `tiny`, `normal`, and `deep`.

Cards are synrepo's native compact delivery packet for code artifacts and task contexts. Use compact search to route, then cards or context packs for bounded detail, then full source only when the bounded context is insufficient.

Default to `tiny`.

## Context budget contract

Do not maximize returned context. Return the smallest useful MCP response.

Default sequence:

1. `synrepo_orient`
2. `synrepo_ask(...)` for plain-language tasks that need a task-context packet
3. `synrepo_task_route(...)` when only route classification is needed
4. `synrepo_search(..., output_mode: "compact", limit: 5-10)` for exact identifiers or drill-down
5. `synrepo_card(..., budget: "tiny")`
6. `synrepo_card(..., budget: "normal")` only for the best 1-3 targets
7. `synrepo_minimum_context(..., budget: "normal")` when local neighborhood matters
8. `synrepo_context_pack(...)` only after targets are known
9. `budget: "deep"` or full-file reads only immediately before implementation or validation

Consume these fields first:

- `synrepo_orient`: `workflow`, `capability_actions`, `graph`, `watch`, `reconcile`
- `synrepo_ask`: `answer`, `cards_used`, `evidence`, `grounding`, `omitted_context_notes`, `next_best_tools`, `context_packet`
- `synrepo_search`: `suggested_card_targets`, `file_groups` or `results`, `miss_reason`, `output_accounting`
- `synrepo_card`: `path`, `symbols`, `exports`, `imports`, `context_accounting`, `commentary_state`
- `synrepo_context_pack`: `artifacts[].target`, `artifacts[].status`, `totals`, `omitted`, `context_state`
- `synrepo_resume_context`: `context_state`, `sections.changed_files`, `sections.next_actions`, `detail_pointers`, `omitted`
- `synrepo_metrics`: `persisted.mcp_tool_errors_total`, `persisted.mcp_tool_error_codes_total`, `persisted.largest_response_tokens`

For `synrepo_ask.evidence`, prefer `source_store`, `confidence`, and `provenance` when deciding trust. `span` is a backward-compatible primary line span; `spans` is the full list. Null or empty spans mean unavailable or disabled, not implicit proof.

Rules:

- Always pass `limit` on search/list tools.
- Prefer `budget_tokens` when available.
- Prefer `output_mode: "compact"` for routing.
- Do not call `output_mode: "cards"` with broad queries.
- Do not batch unrelated targets into one context pack.
- Do not request `deep` cards for more than 1-3 files at a time.
- Do not paste whole MCP JSON responses into reasoning unless the relevant field is needed.
- Treat `context_accounting.truncation_applied: true` as a signal to narrow the query, not automatically escalate.
- If a response includes `omitted`, `truncated`, `output_accounting`, or `context_accounting`, use that metadata to decide the next smallest step.

## Structured MCP errors

MCP errors are structured. Branch on `error.code` when present and use `error_message` only as a compatibility fallback.

Read/card tools are rate-limited. If you receive `RATE_LIMITED`, wait briefly or reduce batching. If you receive `BUSY`, retry after the current read pressure clears.
