# Editing And Mutation

## MCP edit path

When the server was explicitly started with `synrepo mcp --allow-source-edits`, prefer read tools first, then call `synrepo_prepare_edit_context` before `synrepo_apply_anchor_edits`.

Without that process flag, source-edit tools are absent. Overlay write tools such as notes and commentary refresh are also absent unless the server was started with `synrepo mcp --allow-overlay-writes`. Config may further restrict mutation, but config alone does not enable mutation tools.

Do not call `synrepo_apply_anchor_edits` without a fresh `synrepo_prepare_edit_context` response for the target range. Prepared anchors are session-scoped operational state.

Do not expect synrepo MCP edit tools to run shell commands. Command execution is unavailable.

## Fast-path routing

Use `synrepo_task_route(task, path?)` or `synrepo task-route <task> [--path <path>]` when you want the cheapest safe route before reading files or asking an LLM to transform code. It returns `intent`, `confidence`, `recommended_tools`, `budget_tier`, `llm_required`, optional `edit_candidate`, stable `signals`, and a short `reason`.

Hook signals mean:

Client-side nudge hooks for Codex and Claude may remind agents to use synrepo before direct grep, read, review, or edit workflows.

- `[SYNREPO_CONTEXT_FAST_PATH]`: prefer compact search, cards, or context packs before cold file reads.
- `[SYNREPO_DETERMINISTIC_EDIT_CANDIDATE] Intent: <intent>`: a narrow mechanical edit may be possible, but prepare anchors first.
- `[SYNREPO_LLM_NOT_REQUIRED]`: structural context or anchored edits should be enough for the next step.

V1 edit candidates are advisory only: `var-to-const`, `remove-debug-logging`, `replace-literal`, and `rename-local`. Reject `add-types`, `add-error-handling`, `async-await`, and broad rename as LLM-required unless the user supplies a prepared anchor range. Hooks never apply edits.
