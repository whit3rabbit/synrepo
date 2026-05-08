# MCP Tools

Use these as the primary read interface when the synrepo MCP server is available.

## Core tools

- `synrepo_overview()`: full dashboard for graph, readiness, watch/reconcile, explain/commentary, metrics, and recent activity.
- `synrepo_readiness()`: cheap read-only preflight for graph, overlay, index, watch, reconcile, and enabled MCP mutation modes.
- `synrepo_orient()`: compact first-call routing summary.
- `synrepo_ask(ask, scope?, shape?, ground?, budget?)`: default high-level front door for a bounded, cited task-context packet. Use exact search after it when the packet is insufficient or the task names literal identifiers.
- `synrepo_find(task, limit?, budget_tokens?)`: task-oriented routing for plain-language questions. Best for "where should I look?" Not the best first tool for exact symbols, string literals, flags, schema fields, tool names, or file paths.
- `synrepo_where_to_edit(task, limit?)`: ranked edit candidates for plain-language edit tasks. Inspect diagnostics and switch to exact search when broad routing misses.
- `synrepo_search(query, limit?, output_mode?, budget_tokens?)`: lexical search. Best for exact symbols, string literals, CLI flags, MCP tool names, schema keys, file paths, and code-review validation. Use `output_mode: "compact"` for adaptive compact output with `output_accounting`; use `output_mode: "cards"` to return tiny file cards directly.
- `synrepo_explain(target, budget?, budget_tokens?)`: workflow alias for bounded card lookup.
- `synrepo_card(target?, targets?, budget?, budget_tokens?)`: card for one symbol/file, or up to 10 cards in one batch.
- `synrepo_minimum_context(target, budget?)`: bounded neighborhood step before deep inspection or full-file reads.
- `synrepo_context_pack(goal?, targets?, budget?, budget_tokens?, output_mode?, include_tests?, include_notes?, limit?)`: batch read-only context artifacts into one token-accounted response; target kinds include `file`, `symbol`, `directory`, `minimum_context`, `test_surface`, `call_path`, `search`, `entrypoints`, `public_api`, `change_risk`, `findings`, and `recent_activity`; compact mode applies to search artifacts.
- `synrepo_impact(target, budget?, budget_tokens?)`: workflow alias for risk before editing.
- `synrepo_risks(target, budget?, budget_tokens?)`: shorthand for `synrepo_impact`.
- `synrepo_change_impact(target, direction?)`: first-pass dependents and optional outbound dependencies.
- `synrepo_change_risk(target)`: composite risk signal.
- `synrepo_tests(scope, budget?, budget_tokens?)`: workflow alias for test discovery.
- `synrepo_test_surface(scope)`: test discovery.
- `synrepo_changed()`: workflow alias for changed-context review.
- `synrepo_resume_context(limit?, since_days?, budget_tokens?, include_notes?)`: advisory repo-scoped resume packet for stale work, assembled from existing repo state without prompt logs, chat history, or raw tool-output capture.
- `synrepo_task_route(task, path?)`: classify a task into the cheapest safe route and stable hook signals.
- `synrepo_docs_search(query, limit?)`: advisory commentary search.
- `synrepo_notes(include_hidden?)`: read advisory overlay notes.
- `synrepo_refactor_suggestions(min_lines?, limit?, path_filter?)`: large non-test source files with modularity hints.
- `synrepo_entrypoints(scope?, budget?)`: entrypoint discovery.
- `synrepo_metrics()`: this-session and persisted MCP/context metrics.
- `synrepo_use_project(repo_root)`: set the default repo for a global/defaultless MCP session.

## Overlay-write tools

These are present only under `synrepo mcp --allow-overlay-writes`:

- `synrepo_refresh_commentary(scope, target?)`: explicitly generate or refresh advisory commentary.
- `synrepo_note_add(...)`, `synrepo_note_link(...)`, `synrepo_note_supersede(...)`, `synrepo_note_forget(...)`, `synrepo_note_verify(...)`: mutate advisory overlay notes.

## Source-edit tools

These are present only under `synrepo mcp --allow-source-edits`:

- `synrepo_prepare_edit_context(target, target_kind?, start_line?, end_line?, task_id?, budget_lines?)`: prepare session-scoped line anchors and compact source context.
- `synrepo_apply_anchor_edits(edits, diagnostics_budget?)`: validate prepared anchors and apply atomic cross-file edits.
