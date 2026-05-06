# MCP

`synrepo mcp` serves repository context to MCP-compatible coding agents over stdio. It is the agent-facing delivery surface for graph-backed code artifacts, task contexts, cards, search, impact analysis, test discovery, advisory overlay data, optional saved-context note writes, and optional anchored source edits.

MCP is read-only by default. Overlay write tools are hidden unless the server process starts with `synrepo mcp --allow-overlay-writes`. Source edit tools are hidden unless the server process starts with `synrepo mcp --allow-source-edits`. Advisory note tools can mutate only the overlay store and are explicit saved-context actions, not automatic session memory.

## Run

```bash
synrepo mcp                    # stdio server for the current repo
synrepo mcp --repo <path>      # stdio server for a specific repo
synrepo mcp --allow-overlay-writes # expose overlay note/commentary writes
synrepo mcp --allow-source-edits   # expose anchored source edit tools
synrepo mcp --call-timeout 45s # cap read/resource calls, default 30s
```

Most users should prefer `synrepo setup <tool>`, which writes the agent instructions or skill and registers MCP through `agent-config` for supported integrations. The default is global agent config with `synrepo mcp`; pass `--project` to write repo-local MCP config that launches `synrepo mcp --repo .`. Global MCP is lazy: each tool call must supply a registered repository via `repo_root` unless the server has a default repository. Agents can call `synrepo_use_project` once to set a session default for registered projects. Shim-only integrations still need their own MCP config pointed at `synrepo mcp --repo .`.

Codex and Claude can also install local client-side nudges with `synrepo setup codex --agent-hooks` or `synrepo setup claude --agent-hooks`. These hooks call `synrepo agent-hook nudge`, remind the agent to use synrepo before direct grep/read/review/edit workflows, and never block tools or store prompt content.

Nudge output may include structured fast-path signals. `[SYNREPO_CONTEXT_FAST_PATH]` means use compact search, cards, or context packs before cold reads. `[SYNREPO_DETERMINISTIC_EDIT_CANDIDATE] Intent: <intent>` means a narrow mechanical edit may be possible after preparing anchors. `[SYNREPO_LLM_NOT_REQUIRED]` means structural context or anchored edits should be enough for the next step.

`synrepo mcp` does not start `synrepo watch`, install Git hooks, install agent nudge hooks, scan every repository, intercept external tool calls, or keep state fresh in the background. Use `synrepo watch`, `synrepo watch --daemon`, `synrepo install-hooks`, or explicit `synrepo setup <tool> --agent-hooks` when you want those behaviors.

## Default Agent Workflow

The default path for codebase questions, file reviews, broad search, impact checks, and edits is deliberately small first:

1. `synrepo_orient` before reading the repo cold. It is a routing summary, not the full dashboard.
2. `synrepo_ask` for a bounded, cited task-context packet when the task is plain-language or workflow-shaped.
3. `synrepo_find` or `synrepo_search` to drill down. `synrepo_find` decomposes broad task language into deterministic lexical anchors before returning empty.
4. `synrepo_explain` for bounded details on a file or symbol.
5. `synrepo_minimum_context` when a focal target is known but surrounding risk is unclear.
6. `synrepo_impact` or `synrepo_risks` before edits or risky file reviews.
7. `synrepo_tests` before claiming done.
8. `synrepo_changed` after edits to review changed context and validation commands.

Use `tiny` budgets to route, `normal` budgets to understand a neighborhood, and `deep` budgets only before implementation or when exact source details matter. Use `synrepo_readiness` for cheap operational health, `synrepo_task_route` for cheap route classification, and `synrepo_overview` only when the full dashboard is useful. Use `synrepo_context_pack` when batching several known read-only code artifacts or task-context pieces is cheaper than serial tool calls. Its `targets` parameter is an array of structured objects: `{ "kind": "file|symbol|directory|minimum_context|test_surface|call_path|search|entrypoints|public_api|change_risk|findings|recent_activity", "target": "...", "budget": "tiny|normal|deep" }`. For task-context artifacts, `entrypoints` uses `target: "."` for whole-repo scope or a path prefix, `public_api` uses a directory path, `change_risk` uses a file path or symbol, `findings` uses `target: "all"` for bounded unfiltered overlay audit findings, and `recent_activity` uses `target: "release_readiness"` for release-relevant operational activity.

`synrepo_ask` accepts `ask`, optional `scope: { paths, symbols, change_set }`, optional `shape.sections`, optional `ground: { mode|citations, include_spans, allow_overlay }`, and optional `budget: { max_tokens, max_files, max_symbols, freshness, tier }`. It deterministically infers a built-in recipe such as `explain_symbol`, `trace_call`, `review_module`, `security_review`, `release_readiness`, or `fix_test`, compiles it into existing context-pack targets, and returns `{ answer, cards_used, evidence, grounding, omitted_context_notes, next_best_tools, context_packet }`. Evidence entries include `claim`, `source`, `span`, `spans`, `source_store`, `confidence`, and `provenance`, where `span` is `null` and `spans` is empty when line spans are unavailable or `ground.include_spans=false`. Recipes compose existing deterministic cards and searches: symbol explanation adds symbol, minimum-context, and call-path artifacts; trace uses call paths and entrypoints; module review uses module, public API, entrypoints, and scoped risk; security review adds risky-flow lexical probes; release readiness adds recent activity and, only when overlay is allowed, findings; fix-test adds test surface and target context. It is read-only and does not let LLM output mutate graph facts.

Use `synrepo_task_route` before ambiguous or hook-triggered work. It returns `{ intent, confidence, recommended_tools, budget_tier, llm_required, edit_candidate, signals, reason, routing_strategy, semantic_score? }` and records only aggregate counters. Deterministic safety guards always run first, including unsupported transforms and exact mechanical edit candidates. When `semantic-triage` is compiled, enabled, and local assets already load, routing uses semantic intent matching after those guards. Otherwise `routing_strategy` is `keyword_fallback` and `semantic_score` is absent. It does not read source, call an LLM, download a model, or apply edits. The CLI equivalent is `synrepo task-route <task> [--path <path>] [--json]`.

`synrepo_find` and `synrepo_where_to_edit` route plain-language tasks to tiny file cards. They are best for task routing, not exact code symbols, string literals, flags, schema fields, tool names, or file paths. They first try the task text as-is, then use bounded deterministic fallback queries over phrase, token, and snake_case variants. Responses include `query_attempts`, `fallback_used`, and `miss_reason` (`no_index_matches` or `matches_not_in_graph`) so agents can see whether routing failed because the index had no matches or because matched paths were unavailable in the graph. On misses, responses may include `recommended_next_queries` and `recommended_tool: "synrepo_search"` to turn broad failures into exact follow-up probes. If the task includes exact identifiers, call `synrepo_search` first.

`synrepo_search` is read-only search backed by the syntext substrate index and, in `mode: "auto"`, the local vector index when semantic triage is available. It is the first choice for exact symbols, string literals, CLI flags, MCP tool names, schema keys, file paths, and code-review validation. It accepts `query`, optional `mode` (`auto` default, or `lexical`), optional `limit` (default `10`, capped at `50`, with `0` clamped to `1`), optional `path_filter`, optional `file_type`, optional `exclude_type`, optional `case_insensitive` (`ignore_case` is accepted as an alias), optional `output_mode` (`compact` default, `default`, or `cards`), and optional `budget_tokens` (default `1500` for compact or cards output). Compact default responses group matches by file and include `suggested_card_targets`, `omitted`, and `output_accounting`. Explicit `output_mode: "default"` returns bounded raw rows as `results: [{ path, line, content, source, fusion_score, semantic_score?, chunk_id?, symbol_id? }]`. Auto mode fuses lexical and semantic candidates when possible; otherwise it is an explicit lexical fallback with `semantic_available: false`. Semantic-only rows may have `path`, `line`, and `content` as null when the vector row cannot be graph-enriched. Pass `mode: "lexical"` to force exact syntext behavior.

Use compact output for broad searches where routing matters more than raw snippets. Adaptive compact search returns the smallest useful shape: grouped file previews for broad matches, a minimal miss object for zero results, or bounded raw rows for tiny result sets when raw is smaller. Every adaptive compact response includes `output_accounting`; `estimated_tokens_saved` stays at zero whenever the returned shape is not smaller than the original raw rows. Use `output_mode: "cards"` only for narrow searches: effective `limit <= 5` or a `path_filter`. Cards mode dedupes matched graph files, returns tiny file cards, reports unresolved paths, and applies the shared card-set cap when `budget_tokens` is present. `synrepo_context_pack` also accepts `output_mode: "compact"`; only search artifacts use the adaptive compact behavior, while card artifacts retain their existing `context_accounting`.

MCP search is read-only. It searches the persisted substrate indexes as-is and does not reconcile, rebuild indexes, download semantic models, start watch, or silently refresh state. Use `synrepo watch`, `synrepo reconcile`, `synrepo sync`, or initialization flows when you want indexes updated after source changes. Clients should treat `semantic_available` and `routing_strategy` as runtime availability signals, not as configuration promises.

Use `synrepo_readiness` when an agent needs a cheap preflight instead of the full orientation dashboard. It returns `graph`, `overlay`, `index`, `watch`, `reconcile`, and `edit_mode` status fields, plus capability rows with severity and next-action text. It is read-only, never starts watch, never reconciles, and reports whether overlay writes and source edits were enabled for this MCP process.

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
- `synrepo_readiness`
- `synrepo_ask`
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
- `synrepo_findings`
- `synrepo_recent_activity`

Overlay-write tools, present only under `synrepo mcp --allow-overlay-writes`:
- `synrepo_refresh_commentary`
- `synrepo_note_add`
- `synrepo_note_link`
- `synrepo_note_supersede`
- `synrepo_note_forget`
- `synrepo_note_verify`

`synrepo_refresh_commentary` accepts `scope: "target" | "file" | "directory" | "stale"`. `target` preserves one-node refresh behavior. `file` refreshes file commentary plus file symbols. `directory` uses the existing commentary work-plan scoping for that tree. `stale` refreshes stale existing commentary without seeding missing entries. When the MCP client supplies a progress token, the server sends progress notifications around the blocking refresh.

Advisory agent note tools:
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

`synrepo_orient` returns a compact routing summary with graph counts, watch/reconcile state, capability actions, workflow next steps, and a small metrics hint. It intentionally excludes dashboard-only fields such as recent activity, explain provider details, commentary coverage, overlay cost, and edge-kind counts.

`synrepo_overview` keeps the compatibility `mode` and `graph` fields and adds the full orientation dashboard: readiness rows, watch status, reconcile and writer state, export freshness, explain provider state, commentary and overlay state, agent integration readiness, metrics summary, and recent activity. On a global/defaultless server where repository prep fails, overview returns degraded probe data with a structured initialization error rather than failing the whole response.

`synrepo_readiness` returns a compact preflight: `graph: "ready|missing|error"`, `overlay: "ready|missing|error"`, `index: "ready|stale|missing"`, `watch: "active|inactive|starting|stale|error"`, `reconcile: "fresh|stale|missing|error"`, and `edit_mode: { overlay_writes, source_edits }`. The `details.capabilities` array mirrors the readiness matrix used by `synrepo_overview`.

`synrepo_card` accepts either `target` or `targets`. `targets` batches up to 10 files or symbols under one read epoch and returns per-target errors so one missing card does not fail the whole batch. Deep card batches are capped at 3 targets. A single card with `budget_tokens` retries smaller budget tiers before marking truncation; batch calls also apply the token cap to the whole response and report omitted targets. Path-like card targets can return a degraded file stub with existence and Git status when repository prep fails. Mutating tools never degrade. Cards remain the current serialized packet for compiled code artifacts and task contexts; `synrepo_ask` is the high-level task-context front door over those existing packets.

`synrepo_change_impact` accepts `direction: "inbound" | "outbound" | "both"`. The default is `inbound`, preserving the legacy dependent-file response. Outbound adds files reached by imports and calls from the target.

Read-only resources:
- `synrepo://card/{target}`
- `synrepo://file/{path}/outline`
- `synrepo://context-pack?goal={goal}`
- `synrepo://card/src%2Flib.rs?budget=tiny&budget_tokens=1200`
- `synrepo://context-pack?goal=mcp-search&limit=5&budget=tiny&budget_tokens=4000`
- `synrepo://project/{project_id}/card/{target}`
- `synrepo://project/{project_id}/file/{path}/outline`
- `synrepo://project/{project_id}/context-pack?goal={goal}`
- `synrepo://projects`

`synrepo://projects` lists managed project registry entries. Use it with `synrepo_use_project` in global MCP sessions that need to choose a default repository once and omit `repo_root` afterward, or use the project-qualified URI templates with the entry's stable `project_id`.

Default resource URIs use the server default repository only. Global/defaultless resource-aware hosts should call `synrepo_use_project` first, use project-qualified resource URIs, or use tool calls with `repo_root` when addressing non-default projects.

Runtime and observability tools:
- `synrepo_metrics` returns `{ this_session, persisted }`, combining in-memory per-tool call/error counters with persisted context metrics when a repository is available. Persisted metrics include response soft-cap crossings, final truncations, deep-card counts, context-pack token totals, largest response tokens, per-tool token totals, and aggregate error-code counts by tool.
- Simple in-memory limits protect runaway clients: card and context-pack tools allow 10 calls per second, commentary refresh allows 3 calls per minute, and other tools allow 30 calls per second. Rate-limit failures use the structured `RATE_LIMITED` error code.

Source-edit tools, present only under `synrepo mcp --allow-source-edits`:
- `synrepo_prepare_edit_context`
- `synrepo_apply_anchor_edits`

## Errors And Limits

Tool errors are structured as `{"ok":false,"error":{"code":"...","message":"...","retryable":false,"next_action":"..."},"error_message":"..."}`. The transitional `error_message` field keeps legacy message-only clients working while new clients can branch on `error.code`.

`error` is always an object, not a flat string. Clients and tests that need message text should read `error.message` or `error_message`; branch logic should prefer `error.code`.

Current codes are `NOT_FOUND`, `NOT_INITIALIZED`, `INVALID_PARAMETER`, `RATE_LIMITED`, `LOCKED`, `BUSY`, `TIMEOUT`, and `INTERNAL`. Read snapshots are limited per repository, defaulting to 4 concurrent snapshots with a short wait before returning `BUSY`. Per-repo read limiters and SQLite compiler pools are bounded to 128 tracked repositories with idle eviction. Read tools and resource reads are capped by `--call-timeout`, default `30s`, and return `TIMEOUT` on expiry.

Persistent mutating MCP calls are not timed out after their blocking task starts. Source-edit tools, overlay note writes, and commentary refresh complete and return their authoritative outcome rather than reporting `TIMEOUT` while work may still finish in the background.

Output limits apply after every MCP tool handler. The default response cap is 4000 estimated tokens and the hard cap is 12000 estimated tokens. When the final clamp trims a response, it preserves valid JSON, attaches `context_accounting.truncation_applied = true`, and reports omitted fields where possible.

Input limits reject oversized or invalid parameters through `INVALID_PARAMETER`: unknown budget tiers, search queries over 512 characters, card batches over 10 targets, deep card batches over 3 targets, broad `output_mode: "cards"` searches, context packs without targets or a non-empty goal, note claims over 4000 characters, note evidence arrays over 32 entries, note source hash arrays over 32 entries, anchored edit batches over 100 edits, anchored edit batches touching over 20 files, single edit payloads over 256 KiB, and total submitted edit text over 512 KiB.

## Trust Model

Graph-backed structural facts are authoritative. They come from parsers, Git, and human-declared inputs.

Overlay content is advisory. Commentary, explained docs, proposed cross-links, and agent notes are labeled as overlay-backed and freshness-sensitive. If graph facts and overlay prose disagree, trust the graph. If the overlay store becomes unavailable at runtime, graph card responses may continue with `overlay_state: "unavailable"` and `overlay_error`; overlay-backed tools return structured errors instead of silently pretending the overlay is absent.

Freshness is explicit. A stale label is information, not an error, and synrepo does not silently refresh commentary just because an API key exists. Start MCP with `--allow-overlay-writes` and use `synrepo_refresh_commentary` only when fresh advisory prose is required.

Explain credentials and endpoints are operator secrets. Cloud API keys saved by setup live as plaintext in `~/.synrepo/config.toml`; use environment variables instead on shared hosts. Local explain endpoints receive source and context snippets when commentary is refreshed, so do not point MCP-accessible refresh workflows at untrusted local or remote LLM servers.

Prepared edit anchors are short-lived operational state. They are not graph facts, overlay content, commentary, agent notes, canonical source truth, or agent memory.

Context metrics are operational counters only. They track totals such as MCP requests, per-tool calls, per-tool errors, resource reads, card tokens, and explicit note mutations. They never store prompts, queries, note claims, caller identity, or session history.

Fast-path metrics are counters only: route classifications, hook signal emissions, deterministic edit candidates, anchored edit accept/reject totals, and estimated LLM calls avoided. They never store the classified task text or source snippets.

## Edit-Enabled Workflow

Read tools should still come first. When the server was started with `synrepo mcp --allow-source-edits`, use:

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
