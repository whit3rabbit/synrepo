# MCP

`synrepo mcp` serves repository context to MCP-compatible coding agents over stdio. It is the agent-facing surface for cards, search, impact analysis, test discovery, advisory overlay data, explicit saved-context notes, and optional anchored edits.

MCP is source-read-first by default. Source edit tools are hidden unless the server process starts with `synrepo mcp --allow-edits`. Advisory note tools can mutate only the overlay store and are explicit saved-context actions, not automatic session memory.

## Run

```bash
synrepo mcp                    # stdio server for the current repo
synrepo mcp --repo <path>      # stdio server for a specific repo
synrepo mcp --allow-edits      # explicitly expose anchored edit tools
synrepo mcp --call-timeout 45s # cap blocking tool calls, default 30s
```

Most users should prefer `synrepo setup <tool>`, which writes the agent instructions or skill and registers MCP through `agent-config` for supported integrations. The default is global agent config with `synrepo mcp`; pass `--project` to write repo-local MCP config that launches `synrepo mcp --repo .`. Global MCP is lazy: each tool call must supply a registered repository via `repo_root` unless the server has a default repository. Agents can call `synrepo_use_project` once to set a session default for registered projects. Shim-only integrations still need their own MCP config pointed at `synrepo mcp --repo .`.

Codex and Claude can also install local client-side nudges with `synrepo setup codex --agent-hooks` or `synrepo setup claude --agent-hooks`. These hooks call `synrepo agent-hook nudge`, remind the agent to use synrepo before direct grep/read/review/edit workflows, and never block tools or store prompt content.

Nudge output may include structured fast-path signals. `[SYNREPO_CONTEXT_FAST_PATH]` means use compact search, cards, or context packs before cold reads. `[SYNREPO_DETERMINISTIC_EDIT_CANDIDATE] Intent: <intent>` means a narrow mechanical edit may be possible after preparing anchors. `[SYNREPO_LLM_NOT_REQUIRED]` means structural context or anchored edits should be enough for the next step.

`synrepo mcp` does not start `synrepo watch`, install Git hooks, install agent nudge hooks, scan every repository, intercept external tool calls, or keep state fresh in the background. Use `synrepo watch`, `synrepo watch --daemon`, `synrepo install-hooks`, or explicit `synrepo setup <tool> --agent-hooks` when you want those behaviors.

## Default Agent Workflow

The default path for codebase questions, file reviews, broad search, impact checks, and edits is deliberately small first:

1. `synrepo_orient` before reading the repo cold.
2. `synrepo_find` or `synrepo_search` to route a task. `synrepo_find` decomposes broad task language into deterministic lexical anchors before returning empty.
3. `synrepo_explain` for bounded details on a file or symbol.
4. `synrepo_minimum_context` when a focal target is known but surrounding risk is unclear.
5. `synrepo_impact` or `synrepo_risks` before edits or risky file reviews.
6. `synrepo_tests` before claiming done.
7. `synrepo_changed` after edits to review changed context and validation commands.

Use `tiny` budgets to route, `normal` budgets to understand a neighborhood, and `deep` budgets only before implementation or when exact source details matter. Use `synrepo_context_pack` when batching several read-only context artifacts is cheaper than serial tool calls. Its `targets` parameter is an array of structured objects: `{ "kind": "file|symbol|directory|minimum_context|test_surface|call_path|search", "target": "...", "budget": "tiny|normal|deep" }`.

Use `synrepo_task_route` before ambiguous or hook-triggered work. It returns `{ intent, confidence, recommended_tools, budget_tier, llm_required, edit_candidate, signals, reason, routing_strategy, semantic_score? }` and records only aggregate counters. When `semantic-triage` is compiled, enabled, and local assets load, routing uses semantic intent matching after deterministic safety guards. Otherwise `routing_strategy` is `keyword_fallback`. It does not read source, call an LLM, download a model, or apply edits. The CLI equivalent is `synrepo task-route <task> [--path <path>] [--json]`.

`synrepo_find` and `synrepo_where_to_edit` route plain-language tasks to tiny file cards. They first try the task text as-is, then use bounded deterministic fallback queries over phrase, token, and snake_case variants. Responses include `query_attempts`, `fallback_used`, and `miss_reason` (`no_index_matches` or `matches_not_in_graph`) so agents can see whether routing failed because the index had no matches or because matched paths were unavailable in the graph.

`synrepo_search` is read-only search backed by the syntext substrate index and, in `mode: "auto"`, the local vector index when semantic triage is available. It accepts `query`, optional `mode` (`auto` default, or `lexical`), optional `limit` (default `20`), optional `path_filter`, optional `file_type`, optional `exclude_type`, optional `case_insensitive` (`ignore_case` is accepted as an alias), optional `output_mode` (`default`, `compact`, or `cards`), and optional `budget_tokens` for compact or cards output. Default responses preserve `query` and `results: [{ path, line, content, source, fusion_score, semantic_score? }]`, and include `engine`, `source_store`, `mode`, `semantic_available`, `limit`, `filters`, and `result_count`. Semantic-only rows may have `line` and `content` as null. Pass `mode: "lexical"` to force exact syntext behavior.

Use `output_mode: "compact"` for broad searches where routing matters more than raw snippets. Compact search groups matches by file, returns short line previews, includes `suggested_card_targets`, and attaches `output_accounting` with returned tokens, original tokens, estimated savings, omitted count, and truncation state. Use `output_mode: "cards"` when the next step would be `synrepo_card` for each matched file. Cards mode dedupes matched graph files, returns tiny file cards, reports unresolved paths, and applies the shared card-set cap when `budget_tokens` is present. `synrepo_context_pack` also accepts `output_mode: "compact"`; only search artifacts switch to compact output, while card artifacts retain their existing `context_accounting`.

MCP search is read-only. It searches the persisted substrate indexes as-is and does not reconcile, rebuild indexes, download semantic models, start watch, or silently refresh state. Use `synrepo watch`, `synrepo reconcile`, `synrepo sync`, or initialization flows when you want indexes updated after source changes.

## Tool Groups

Workflow aliases:
- `synrepo_orient`
- `synrepo_find`
- `synrepo_explain`
- `synrepo_impact`
- `synrepo_risks`
- `synrepo_tests`
- `synrepo_changed`

Task-first read tools:
- `synrepo_overview`
- `synrepo_card`
- `synrepo_context_pack`
- `synrepo_task_route`
- `synrepo_search`
- `synrepo_where_to_edit`
- `synrepo_refactor_suggestions`
- `synrepo_change_impact`
- `synrepo_change_risk`
- `synrepo_entrypoints`
- `synrepo_test_surface`
- `synrepo_module_card`
- `synrepo_public_api`
- `synrepo_minimum_context`
- `synrepo_call_path`
- `synrepo_next_actions`
- `synrepo_metrics`
- `synrepo_use_project`

Advisory overlay and audit tools:
- `synrepo_docs_search`
- `synrepo_refresh_commentary`
- `synrepo_findings`
- `synrepo_recent_activity`

`synrepo_refresh_commentary` accepts `scope: "target" | "file" | "directory" | "stale"`. `target` preserves one-node refresh behavior. `file` refreshes file commentary plus file symbols. `directory` uses the existing commentary work-plan scoping for that tree. `stale` refreshes stale existing commentary without seeding missing entries. When the MCP client supplies a progress token, the server sends progress notifications around the blocking refresh.

Advisory agent note tools:
- `synrepo_note_add`
- `synrepo_note_link`
- `synrepo_note_supersede`
- `synrepo_note_forget`
- `synrepo_note_verify`
- `synrepo_notes`

Low-level graph primitives:
- `synrepo_node`
- `synrepo_edges`
- `synrepo_query`
- `synrepo_graph_neighborhood`
- `synrepo_overlay`
- `synrepo_provenance`

`synrepo_query` accepts the same target forms as `synrepo graph query`: node IDs (`file_...`, `sym_...`, `concept_...`), file paths, qualified symbol names, and short symbol names. `symbol_...` is accepted only as a legacy input alias; responses use canonical `sym_...` IDs.

Use `synrepo_graph_neighborhood` when an agent needs a bounded graph-shaped response directly. Defaults are `direction: "both"`, `depth: 1`, and `limit: 100`; depth is clamped to `3`, limit to `500`, and `target: null` returns a deterministic top-degree overview.

Use `synrepo_refactor_suggestions` when an agent or operator asks whether large files should be refactored. It is read-only and returns deterministic facts for non-test source files over a physical-line threshold: graph file IDs, paths, language labels, line counts, symbol counts, modularity tags, and suggested follow-up MCP tools. The response is labeled `source_store: "graph+filesystem"` because it combines graph metadata with current filesystem line counts; it does not generate edits or refactor plans.

`synrepo_overview` keeps the compatibility `mode` and `graph` fields and adds an orientation dashboard: readiness rows, watch status, reconcile and writer state, export freshness, explain provider state, commentary and overlay state, agent integration readiness, metrics summary, and recent activity. On a global/defaultless server where repository prep fails, overview returns degraded probe data with a structured initialization error rather than failing the whole response.

`synrepo_card` accepts either `target` or `targets`. `targets` batches up to 10 files or symbols under one read epoch and returns per-target errors so one missing card does not fail the whole batch. A single card with `budget_tokens` retries smaller budget tiers before marking truncation. Path-like card targets can return a degraded file stub with existence and Git status when repository prep fails. Mutating tools never degrade.

`synrepo_change_impact` accepts `direction: "inbound" | "outbound" | "both"`. The default is `inbound`, preserving the legacy dependent-file response. Outbound adds files reached by imports and calls from the target.

Read-only resources:
- `synrepo://card/{target}`
- `synrepo://file/{path}/outline`
- `synrepo://context-pack?goal={goal}`
- `synrepo://projects`

`synrepo://projects` lists managed project registry entries. Use it with `synrepo_use_project` in global MCP sessions that need to choose a default repository once and omit `repo_root` afterward.

Runtime and observability tools:
- `synrepo_metrics` returns `{ this_session, persisted }`, combining in-memory per-tool call/error counters with persisted context metrics when a repository is available.
- Simple in-memory limits protect runaway clients: card and context-pack tools allow 10 calls per second, commentary refresh allows 3 calls per minute, and other tools allow 30 calls per second. Rate-limit failures use the structured `RATE_LIMITED` error code.

Edit-enabled tools, present only under `synrepo mcp --allow-edits`:
- `synrepo_prepare_edit_context`
- `synrepo_apply_anchor_edits`

## Errors And Limits

Tool errors are structured as `{"error":{"code":"...","message":"..."},"error_message":"..."}`. The transitional `error_message` field keeps legacy message-only clients working while new clients can branch on `error.code`.

Current codes are `NOT_FOUND`, `NOT_INITIALIZED`, `INVALID_PARAMETER`, `RATE_LIMITED`, `LOCKED`, `BUSY`, `TIMEOUT`, and `INTERNAL`. Read snapshots are limited per repository, defaulting to 4 concurrent snapshots with a short wait before returning `BUSY`. Blocking tools are capped by `--call-timeout`, default `30s`, and return `TIMEOUT` on expiry.

Input limits reject oversized parameters through `INVALID_PARAMETER`: search queries over 1000 characters, note claims over 2000 characters, note evidence arrays over 50 entries, note source hash arrays over 50 entries, and card batches over 10 targets.

## Trust Model

Graph-backed structural facts are authoritative. They come from parsers, Git, and human-declared inputs.

Overlay content is advisory. Commentary, explained docs, proposed cross-links, and agent notes are labeled as overlay-backed and freshness-sensitive. If graph facts and overlay prose disagree, trust the graph.

Freshness is explicit. A stale label is information, not an error, and synrepo does not silently refresh commentary just because an API key exists. Use `synrepo_refresh_commentary` only when fresh advisory prose is required.

Explain credentials and endpoints are operator secrets. Cloud API keys saved by setup live as plaintext in `~/.synrepo/config.toml`; use environment variables instead on shared hosts. Local explain endpoints receive source and context snippets when commentary is refreshed, so do not point MCP-accessible refresh workflows at untrusted local or remote LLM servers.

Prepared edit anchors are short-lived operational state. They are not graph facts, overlay content, commentary, agent notes, canonical source truth, or agent memory.

Context metrics are operational counters only. They track totals such as MCP requests, per-tool calls, per-tool errors, resource reads, card tokens, and explicit note mutations. They never store prompts, queries, note claims, caller identity, or session history.

Fast-path metrics are counters only: route classifications, hook signal emissions, deterministic edit candidates, anchored edit accept/reject totals, and estimated LLM calls avoided. They never store the classified task text or source snippets.

## Edit-Enabled Workflow

Read tools should still come first. When the server was started with `synrepo mcp --allow-edits`, use:

1. `synrepo_prepare_edit_context` to prepare session-scoped line anchors and compact source context.
2. `synrepo_apply_anchor_edits` to validate prepared anchors, content hashes, and boundary text before writing.

File batches are atomic per file and across files. Multi-file calls preflight every file before writing. If any later write fails, prior originals are restored with atomic writes and the response reports rollback status. The response sets `atomicity.cross_file: true`. The edit tools do not run shell commands.

## CLI Fallback

If MCP is unavailable, use the CLI rather than blocking:

```bash
synrepo status
synrepo status --recent
synrepo task-route "find auth entrypoints"
synrepo search "term"
synrepo node <target>
synrepo graph query "inbound <target>"
synrepo graph query "outbound <target>"
synrepo graph view <target> --json
synrepo graph view <target>          # TTY-only terminal explorer
synrepo graph stats
synrepo reconcile
synrepo findings
```

If neither MCP nor the CLI is available, fall back to normal file reading.
