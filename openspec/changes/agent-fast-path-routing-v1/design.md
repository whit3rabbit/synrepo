## Context

synrepo already has the substrate for LLM avoidance: structural cards, compact search, context packs, and gated anchored edits. The missing piece is an explicit routing signal that agents can consume before they spend tokens on cold source reads or broad reasoning.

The existing `agent-nudge-hooks-v1` implementation is the right integration point for Codex and Claude. The MCP server remains read-first and non-intercepting.

## Goals / Non-Goals

Goals:
- Add a deterministic classifier shared by hooks, MCP, and CLI.
- Emit concise, structured signals from hooks while preserving nudge-only behavior.
- Track aggregate usage metrics without storing prompts, queries, source snippets, or caller identity.
- Keep deterministic edit scope conservative and recommendation-only.

Non-goals:
- No WebAssembly runtime.
- No repo-wide rewrite tool.
- No LLM model router or provider switching.
- No task memory, swarms, federation, or self-learning agent orchestration.
- No automatic application of edit candidates from hook output.

## Design

Add `src/surface/task_route.rs` as the shared classifier. It accepts a task string and an optional source path, returns a serializable route result with:
- `intent`
- `confidence`
- `recommended_tools`
- `budget_tier`
- `llm_required`
- `edit_candidate`
- `signals`
- `reason`

The classifier is intentionally string-and-extension based for routing, not rewriting. V1 recognizes context/search/review/test/risk workflows and conservative edit candidate intents:
- `var-to-const`
- `remove-debug-logging`
- `replace-literal`
- `rename-local`

Only `var-to-const` gets a parser-backed preview helper for TypeScript/TSX source snippets. That preview must prove no reassignment after the declaration before reporting `eligible = true`. Unsupported higher-semantics requests such as adding types, async conversion, and error handling are classified as LLM-required.

Expose the classifier through:
- MCP tool `synrepo_task_route`
- CLI command `synrepo task-route <task> [--path <path>] [--json]`
- hook signals embedded into the existing Codex/Claude nudge output

Context metrics get append-only serde-default counters. Hook and route calls record counters in the current repo's `.synrepo/state/context-metrics.json` only when the repo can be resolved. No task text is persisted.

## Risks / Trade-offs

- String classification can false-positive. Mitigation: emit recommendations only, keep confidence visible, and keep source mutation behind anchored edits.
- Hook payloads may not include repo root. Mitigation: record metrics best-effort and still emit stateless signals.
- Parser-backed edit eligibility can be incomplete. Mitigation: V1 only proves a narrow no-reassignment case and returns ineligible for ambiguity.
- Additional MCP/CLI surface increases docs burden. Mitigation: update the canonical doctrine, MCP docs, and skill guidance in the same change.
