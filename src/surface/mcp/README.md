# synrepo MCP server

Front-door product docs live in [`README.md`](../../../README.md) and [`docs/MCP.md`](../../../docs/MCP.md).
This file stays focused on the MCP surface exposed by `synrepo mcp`: handler layout, tool registration, and invariants.

## Run

```bash
cargo run -- mcp                    # stdio server for the current repo
cargo run -- mcp --repo <path>      # stdio server for a specific repo
cargo run -- mcp --allow-overlay-writes # expose overlay note/commentary writes
cargo run -- mcp --allow-source-edits   # expose anchored source edit tools
cargo run -- mcp --call-timeout 45s # cap read/resource calls, default 30s
```

The server is a subcommand of the `synrepo` binary. There is no separate crate. Transport is stdio only.

MCP is read-only by default. Overlay write tools are absent from `tools/list` unless the process was started with `synrepo mcp --allow-overlay-writes`. Source edit tools are absent unless the process was started with `synrepo mcp --allow-source-edits`. Repository or user configuration may further restrict editing later, but config alone must never enable mutating tools.

## Where things live

| Concern | Location |
|---------|----------|
| Tool registration, JSON schemas, dispatch | `src/bin/cli_support/commands/mcp.rs` |
| Per-request handler logic | `src/surface/mcp/*.rs` (this directory) |
| Shared state (`SynrepoState`, snapshot helpers) | `src/surface/mcp/mod.rs` |
| Public MCP guide | `docs/MCP.md` |
| Agent-facing protocol doc | `skill/SKILL.md` (repo root) |
| Spec | `openspec/specs/mcp-surface/spec.md` |

Keep `src/bin/cli_support/commands/mcp.rs` as the single registration file. The agent-doctrine source scan test (`src/bin/cli_support/agent_shims/tests.rs`) reads this path and will fail if tools are registered elsewhere.

## Tools

Current registrations (see `mcp.rs` for schemas):

**Workflow aliases:**
`synrepo_orient`, `synrepo_find`, `synrepo_explain`, `synrepo_impact`, `synrepo_risks`, `synrepo_tests`, `synrepo_changed`, `synrepo_resume_context`

**Task-first read tools:**
`synrepo_overview`, `synrepo_readiness`, `synrepo_ask`, `synrepo_card`, `synrepo_context_pack`, `synrepo_search`, `synrepo_resume_context`, `synrepo_where_to_edit`, `synrepo_refactor_suggestions`, `synrepo_change_impact`, `synrepo_change_risk`, `synrepo_entrypoints`, `synrepo_test_surface`, `synrepo_module_card`, `synrepo_public_api`, `synrepo_minimum_context`, `synrepo_call_path`, `synrepo_next_actions`, `synrepo_metrics`, `synrepo_use_project`

`synrepo_ask` is the high-level task-context front door. It accepts `ask`, optional `scope {paths,symbols,change_set}`, optional `shape.sections`, optional `ground {mode|citations,include_spans,allow_overlay}`, and optional `budget {max_tokens,max_files,max_symbols,freshness,tier}`. It infers a deterministic built-in recipe, compiles it to context-pack targets, and returns `answer`, `cards_used`, `evidence`, `grounding`, `omitted_context_notes`, `next_best_tools`, and `context_packet`. Each evidence entry carries `claim`, `source`, `span`, `spans`, `source_store`, `confidence`, and `provenance`; missing or disabled spans stay explicit instead of being guessed. The built-ins are `explain_symbol`, `trace_call`, `review_module`, `security_review`, `release_readiness`, and `fix_test`. They compose existing deterministic cards, searches, audit, and recent-activity artifacts. Overlay-backed findings are included only when `ground.allow_overlay` is true. The tool is read-only and never promotes overlay output into graph truth.

`synrepo_context_pack` batches known artifacts with structured targets `{kind,target,budget?}`. Supported kinds are `file`, `symbol`, `directory`, `minimum_context`, `test_surface`, `call_path`, `search`, `entrypoints`, `public_api`, `change_risk`, `findings`, and `recent_activity`. `entrypoints` accepts `target: "."` for whole repo or a path prefix; `public_api` accepts a directory; `change_risk` accepts a file path or symbol; `findings` accepts `target: "all"`; `recent_activity` accepts `target: "release_readiness"` for release-oriented activity.

`synrepo_search` is backed by the syntext substrate index for the primary checkout and a bounded direct scan for discovered non-primary roots such as linked worktrees. It accepts `query`, optional `literal`, `limit`, `path_filter`, `file_type`, `exclude_type`, `case_insensitive` (`ignore_case` alias), `output_mode`, and `budget_tokens`. It is the first choice for exact symbols, string literals, CLI flags, MCP tool names, schema keys, file paths, and code-review validation. Set `literal: true` for code strings that contain regex metacharacters. If a regex query fails to compile, search retries as an escaped literal and returns `pattern_mode: "literal_fallback"` plus `warnings`; normal regex and explicit literal responses return `pattern_mode: "regex"` or `"literal"`. Default output is adaptive compact, with limit 10 and budget 1500. `output_mode="compact"` groups previews by `(root_id, path)` and includes root-aware `suggested_card_requests` alongside compatibility `suggested_card_targets`, or returns a minimal miss or smaller raw rows with `output_accounting`. `output_mode="default"` returns bounded raw rows with `path`, `root_id`, `is_primary_root`, and `file_id` when graph lookup succeeds. `output_mode="cards"` requires a narrow request (`limit <= 5` or `path_filter`) and returns tiny file cards with unresolved-path diagnostics. Search never mutates or refreshes the index during the call.

`synrepo_card` accepts `target` or `targets`. Batch requests are capped at 10 targets, deep batches are capped at 3 targets, run under one read epoch, and return per-target errors. Existing directory targets return `module_card`; existing repo text files that are not graph-backed return bounded `filesystem_fallback` cards with `graph_backed: false`, path, size, headings or preview, and next-step guidance. `FileCard` carries `root_id` and `is_primary_root`; `SymbolCard` also carries `file_id` and `path`. When two roots have the same relative path, callers should use the `file_id` returned by search or the root-aware `suggested_card_requests` entry rather than a bare path. Single-card `budget_tokens` requests retry smaller budget tiers before marking truncation; batch calls default to a 4000-token internal cap, apply the cap to the whole response, and report omitted targets. `synrepo_change_impact` accepts `direction: "inbound" | "outbound" | "both"`, defaulting to inbound.

`synrepo_orient` is the small routing summary for first contact. `synrepo_overview` keeps `mode` and `graph`, then adds readiness, watch, reconcile, export freshness, explain provider, commentary/overlay, agent integration, metrics, and recent activity summaries. Degraded overview and path-like card stubs can be returned when global/defaultless repository prep fails. Mutating tools return initialization errors instead.

`synrepo_readiness` is the cheap read-only preflight for agents that only need operational trust signals. It returns top-level `graph`, `overlay`, `index`, `watch`, `reconcile`, `explain_hint`, and `edit_mode` fields, plus `details.capabilities` rows from the shared readiness matrix. Overlay wire values are `ready`, `ready_empty`, `missing`, and `error`; `ready_empty` is the healthy post-init state before commentary, cross-links, or notes are generated. When overlay commentary exists, `explain_hint` points agents to `synrepo_explain budget=deep` and `synrepo_docs_search`, and reports when refresh is unavailable because `overlay_writes=false`. It does not start watch, run reconcile, refresh commentary, or write overlay/source state.

`synrepo_resume_context` returns a repo-scoped resume packet for stale resumes before an agent asks the user to repeat context. It summarizes changed files, next actions, recent activity, explicit saved note summaries, validation commands, detail pointers, and context accounting under a bounded token budget. It is read-only and regeneratable. It does not store prompt logs, chat history, raw tool outputs, caller identity, response bodies, or generic session memory.

`synrepo_find` and `synrepo_where_to_edit` route plain-language tasks to tiny file cards. They are best for task routing, not broad review synthesis and not exact code symbols, string literals, flags, schema fields, tool names, or file paths. They first try the task text as-is, then decompose broad language into bounded deterministic lexical anchors (phrase, token, and snake_case variants such as `agent_hooks` or `context_metrics`) before returning empty. Responses include `query_attempts`, `fallback_used`, `miss_reason`, and explicit `omitted` diagnostics when the default 4000-token cap drops suggestions; these diagnostics are returned to the caller and are not persisted as task content. On misses, responses may include `recommended_next_queries` and `recommended_tool: "synrepo_search"` for exact follow-up probes. If the task is broad architecture or review work, call `synrepo_ask` first. If the task includes exact identifiers, call `synrepo_search` first.

`synrepo_refactor_suggestions` reports non-test source files over a physical-line threshold, defaulting to files over 300 lines. It combines graph file metadata with current filesystem line counts and returns deterministic modularity hints for downstream LLM analysis; it never writes source or overlay state.

**Advisory overlay and audit read tools:**
`synrepo_docs_search`, `synrepo_findings`, `synrepo_recent_activity`

**Overlay-write only (`synrepo mcp --allow-overlay-writes`):**
`synrepo_refresh_commentary`, `synrepo_note_add`, `synrepo_note_link`, `synrepo_note_supersede`, `synrepo_note_forget`, `synrepo_note_verify`

`synrepo_docs_search` searches existing materialized explain docs. It is read-only and does not generate or refresh commentary.

`synrepo_refresh_commentary` accepts `scope: "target" | "file" | "directory" | "stale"` and emits MCP progress notifications when a client supplies a progress token.

**Advisory agent note tools:**
`synrepo_notes` is read-only and available by default. Note lifecycle mutation tools require `--allow-overlay-writes`.

**Source-edit only (`synrepo mcp --allow-source-edits`):**
`synrepo_prepare_edit_context`, `synrepo_apply_anchor_edits`

**Low-level primitives:**
`synrepo_node`, `synrepo_edges`, `synrepo_query`, `synrepo_overlay`, `synrepo_provenance`

**Resources:**
`synrepo://card/{target}`, `synrepo://file/{path}/outline`, `synrepo://context-pack?goal={goal}`, `synrepo://project/{project_id}/card/{target}`, `synrepo://project/{project_id}/file/{path}/outline`, `synrepo://project/{project_id}/context-pack?goal={goal}`, `synrepo://projects`

Resources are read-only mirrors of tool-backed context. Default resource URIs use the server default repository; global/defaultless hosts should call `synrepo_use_project` first, use project-qualified resources with a stable registry project ID, or prefer tools with `repo_root`. Tool-only hosts should call `synrepo_context_pack`; resource-aware hosts can cache the URI forms. Resource examples with numeric budgets: `synrepo://card/src%2Flib.rs?budget=tiny&budget_tokens=1200` and `synrepo://context-pack?goal=mcp-search&limit=5&budget=tiny&budget_tokens=4000`.

Errors render as `{"ok":false,"error":{"code":"...","message":"...","retryable":false,"next_action":"..."},"error_message":"..."}` with codes `NOT_FOUND`, `NOT_INITIALIZED`, `INVALID_PARAMETER`, `RATE_LIMITED`, `LOCKED`, `BUSY`, `TIMEOUT`, and `INTERNAL`. Context metrics count errors by tool and stable error code without storing targets or queries. The server enforces input limits for search query length, strict budget tiers, note claim/evidence/source-hash sizes, card batch size, and anchored edit payload size/count/file count. It also applies a final response cap: 4000 estimated tokens by default and 12000 estimated tokens hard maximum. In-memory token buckets limit card/context-pack calls to 10 per second, commentary refresh to 3 per minute, and other tools to 30 per second. Read tools and resource reads obey `--call-timeout`; persistent mutating calls complete and return their authoritative outcome once started. Per-repo read limiters and SQLite compiler pools are bounded to 128 tracked repositories with idle eviction.

## Edit-enabled workflow

Use read tools first. When source-edit mode is explicitly enabled, call `synrepo_prepare_edit_context` before `synrepo_apply_anchor_edits`.

`synrepo_prepare_edit_context` accepts file, symbol, and range targets plus optional `root_id`. Omitted `root_id` means the primary checkout. Node-ID targets resolve through the graph file's stored root and reject a mismatched requested root. It returns compact source context plus `task_id`, `anchor_state_version`, `path`, `root_id`, `is_primary_root`, `file_id`, `content_hash`, `source_hash`, and prepared line anchors.

`synrepo_apply_anchor_edits` accepts optional `root_id` on each edit, validates the live anchor session, content hash, anchor existence, optional end-anchor ordering, exact current boundary text, and selected root before writing. Multi-file calls group edits by `(root_id, path)`, preflight every file before writing, and roll back prior originals if a later write fails. Responses set `atomicity.cross_file: true`.

Prepared anchors are session-scoped operational cache entries. They are not graph nodes, overlay content, commentary, agent notes, canonical source truth, or agent memory. Reconcile remains the only producer of graph facts after a write.

Post-edit diagnostics are bounded to built-in validation, write status, reconcile or watch delegation, and test-surface recommendations. The edit tools do not register or run arbitrary command execution.

## Adding or changing a tool

1. Register in `src/bin/cli_support/commands/mcp.rs` (name, params struct, dispatch).
2. Add the handler in this directory, or extend an existing module (`cards.rs`, `search.rs`, `findings.rs`, `audit.rs`, `primitives.rs`, `helpers.rs`).
3. Update `skill/SKILL.md`:
   - Agent-facing tool: add to `## Core tools`.
   - Primitive: leave out of `Core tools` but mention in the `synrepo_overview` description string in `mcp.rs`.
4. Update `docs/MCP.md` when the public tool surface, resource list, workflow, trust model, or edit-gating behavior changes.
5. If the change alters budget tiers, freshness semantics, or trust model, update the corresponding `SKILL.md` sections.

The overview blurb in `mcp.rs`, `skill/SKILL.md`, and `docs/MCP.md` are the surfaces agents and operators see first. They must tell the same story.

## Invariants

- Graph content is primary; overlay is advisory.
- Overlay write tools require an explicit `--allow-overlay-writes` process gate and are hidden by default.
- Source edit tools require an explicit `--allow-source-edits` process gate and are hidden by default.
- Prepared anchors are session-scoped operational state, not canonical graph facts or agent memory.
- `synrepo_docs_search` returns advisory explained commentary only. It is searchable overlay output, not canonical graph state or explain input, and it does not refresh commentary.
- Graph card responses may continue with `overlay_state: "unavailable"` and `overlay_error` when the overlay store cannot be opened. Overlay-backed tools return structured errors in that state.
- Multi-query reads run under `with_graph_read_snapshot` / `with_overlay_read_snapshot`. The re-entrant depth counter lets handlers and card compilers nest snapshots safely (see hard invariant 8 in the root `AGENTS.md`).
- MCP read snapshots are capped per repository so concurrent clients return `BUSY` instead of opening unbounded WAL readers.
- Overlay promotion to graph edges is curated-mode-only.
- Budget tiers: `tiny` → `normal` → `deep`. Default is `tiny`.

## Tests

Handler-level tests live alongside each module. End-to-end MCP request-response tests live under `src/bin/cli_support/tests/` and exercise the binary crate via the dispatch path in `src/bin/cli_support/commands/mcp.rs`.
