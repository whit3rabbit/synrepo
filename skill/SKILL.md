---
name: synrepo
description: Use synrepo in repositories with a .synrepo/ directory. Prefer synrepo cards, compact search, and bounded task contexts before reading source files cold.
---

# synrepo

Use this skill only when the current repository contains a `.synrepo/` directory.

This repo-root file is the canonical agent protocol doc for synrepo. It is not the standalone install target. Installed Agent Skills live in spec-valid directories such as `.agents/skills/synrepo/` and `.claude/skills/synrepo/`.

For product overview, setup flow, and operator-facing docs, start with [`README.md`](../README.md). This file is the agent operating guide for querying synrepo safely once the repo is already wired.

Synrepo is a local, deterministic code-context compiler. Its product model is `repo files -> graph facts -> code artifacts -> task contexts -> cards/MCP`. Graph facts are authoritative observed source truth; code artifacts are compiled records; task contexts are bounded bundles for a workflow; cards and MCP responses are the delivery packets you consume.

Use `synrepo_ask(ask, scope?, shape?, ground?, budget?)` as the default high-level front door for one bounded, cited task-context packet. It returns `answer`, `cards_used`, `evidence`, `grounding`, `omitted_context_notes`, `next_best_tools`, and `context_packet`. Its grounding policy accepts `mode` or `citations`, `include_spans`, and `allow_overlay`; default to graph facts as authoritative observed source truth. Overlay commentary, explain docs, and notes are advisory; LLM-authored output never mutates the canonical graph. Embeddings are optional routing/search helpers.

## Use when

Use synrepo for orientation, codebase questions, reviews, broad search before opening files, exact lexical search, edit routing, first-pass impact, entrypoint discovery, test-surface discovery, and high-level subsystem understanding.

Do not use synrepo for tiny files you already need to edit directly, files already in working context, simple config or text files with no meaningful symbols, or raw-source patching after a bounded synrepo card has already identified the exact range.

## Default path

The required sequence for codebase questions, reviews, search routing, and edits is orient, ask or search, cards, impact or risks, edit, tests, changed.

1. Start with `synrepo_orient` before reading the repo cold. It is a small routing summary, not the full dashboard.
2. Use `synrepo_ask` for broad plain-language tasks that need one bounded, cited task-context packet.
3. Use `synrepo_task_route` when only classification is needed, then choose between `synrepo_find`, `synrepo_where_to_edit`, and `synrepo_search`.
4. Use `tiny` cards to route and `normal` cards to understand. Use `synrepo_minimum_context` as the bounded neighborhood step when a focal target is known but the surrounding risk is unclear, especially for file reviews and codebase questions.
5. Use `synrepo_impact` or `synrepo_risks` before editing or reviewing risky files.
6. Use `synrepo_tests` before claiming done.
7. Use `synrepo_changed` after edits to review changed context and validation commands.
8. After resuming stale work or losing conversation context, call `synrepo_resume_context` before asking the user to repeat repo state.
9. Read full source files or request `deep` cards only after bounded cards identify the target or when the card content is insufficient. Full-file reads are an explicit escalation, not the default first step.

Rule of thumb: `tiny` to find, `normal` to understand, `deep` to write.

## Read next

Load only the reference file that matches the immediate task:

- [`references/search-routing.md`](references/search-routing.md): exact search, broad query recovery, diagnostic fields, and code-shaped probes.
- [`references/mcp-tools.md`](references/mcp-tools.md): full tool catalog, overlay-write tools, source-edit tools, and when to use each.
- [`references/budgets-and-errors.md`](references/budgets-and-errors.md): budget tiers, context contract, response fields, rate limits, and structured errors.
- [`references/editing-and-mutation.md`](references/editing-and-mutation.md): MCP edit path, mutation gates, prepared anchors, and hook signals.
- [`references/setup-and-fallback.md`](references/setup-and-fallback.md): project/global MCP selection, managed project registration, resources, and CLI fallback.

## Search routing

For exact symbols, tool names, function names, flags, JSON keys, CLI args, error strings, or file paths, prefer:

- `synrepo_search(query, limit?, output_mode?, budget_tokens?)`

Use `output_mode: "compact"` for orientation. Do not use a full sentence when an exact token or string literal is known. For plain-language edit or investigation tasks, call:

- `synrepo_ask(ask, scope?, shape?, ground?, budget?)`
- `synrepo_task_route(task, path?)`
- `synrepo_find(task, limit?, budget_tokens?)`
- `synrepo_where_to_edit(task, limit?)`

Read `query_attempts`, `fallback_used`, `miss_reason`, `recommended_next_queries`, `recommended_tool`, `suggested_card_targets`, and `output_accounting` when present. If `miss_reason` is `no_index_matches`, do not retry the same broad sentence. Switch to exact lexical probes.

See [`references/search-routing.md`](references/search-routing.md) for examples, fallback rules, and phrase-to-probe mappings.

## Trust model

Graph content is primary. Overlay content is advisory. Materialized advisory explain docs are overlay output, not canonical graph facts. Prepared edit anchors are session-scoped operational state. They are not graph facts, overlay content, commentary, agent notes, or agent memory.

If overlay and graph disagree, trust the graph. Freshness is explicit. A stale label is information, not an error; it is not silently refreshed on read.

Global MCP configs that launch `synrepo mcp` serve registered projects by absolute path. In global or defaultless contexts, pass the current workspace's absolute path as `repo_root` to repo-addressable tools, or call `synrepo_use_project(repo_root)` once to set the session default.

If a tool reports that a repository is not managed by synrepo, ask the user to run:

```bash
synrepo project add <path>
```

Do not bypass registry gating.

Client-side nudge hooks for Codex and Claude may remind agents to use synrepo before direct grep, read, review, or edit workflows. Do not mistake client-side hook nudges for MCP interception or enforcement. They are non-blocking reminders.

## Do not

- Do not open large files first. Start at `tiny` and escalate only when a specific field forces it.
- Do not treat overlay commentary, explain docs, or proposed cross-links as canonical source truth.
- Do not trigger explain (`--generate-cross-links`, deep commentary refresh) unless the task justifies the cost.
- Do not expect watch or background behavior unless `synrepo watch` is explicitly running.
- Do not call `synrepo_apply_anchor_edits` without a fresh `synrepo_prepare_edit_context` response.
- Do not expect synrepo MCP edit tools to run shell commands. Command execution is unavailable.
- Do not retry the same failed broad `synrepo_find` query repeatedly. Convert it to exact `synrepo_search` probes.
- Do not claim validation from search hits alone. Confirm with cards, source, or tests.
- Do not ask the user to repeat stale repo context until `synrepo_resume_context` has been tried.

## Context budget contract

Do not maximize returned context. Return the smallest useful MCP response.

Default to `tiny`. Use `normal` for the best 1-3 targets when local understanding matters. Use `deep` or full-file reads only immediately before implementation or validation. Do not request `deep` cards for more than 1-3 files at a time.

See [`references/budgets-and-errors.md`](references/budgets-and-errors.md) for the full budget sequence, response fields to consume first, and rate-limit/error handling.

## Product boundary

synrepo stores code facts and bounded operational memory. It is not a task tracker, not session memory, and not cross-session agent memory.

Any handoff or next-action list is a derived recommendation regenerated from repo state. External task systems own assignment, status, and collaboration.

Prepared edit anchors are short-lived MCP operational state, not durable agent memory.

`synrepo_resume_context` is advisory, read-only, and regeneratable. It is not prompt logging, chat history, raw tool-output capture, or generic session memory.
