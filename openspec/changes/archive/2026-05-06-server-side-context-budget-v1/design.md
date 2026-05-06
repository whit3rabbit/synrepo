## Context

Synrepo already has compact search, card budgets, context-pack caps, and aggregate context metrics. Those controls are opt-in or local to specific tools. A malformed or over-broad client request can still ask for raw search rows, large graph primitive responses, large context packs, or deep card batches.

## Goals / Non-Goals

**Goals:**

- Make compact, bounded responses the default MCP behavior.
- Add one final clamp for every tool response that can trim known large fields before returning JSON.
- Keep escalation paths explicit: raw/default search, normal/deep cards, and graph primitives remain available but capped.
- Preserve structured errors and existing trust labels.
- Record aggregate flood metrics without storing query text, snippets, prompts, note bodies, caller identity, or response bodies.

**Non-Goals:**

- No LLM summarization.
- No arbitrary command proxy or command-output filtering inside MCP.
- No graph, overlay, or SQLite schema migration.
- No source mutation changes.

## Decisions

1. Put hard response clamping behind the binary-side MCP dispatch path so every tool benefits, including handlers that do not use `render_result`.
2. Keep tool-local reducers first. The global clamp is a last resort and trims known arrays or card batches rather than slicing raw JSON text.
3. Use a shared deterministic three-bytes-per-token estimator, matching card accounting.
4. Clamp `limit: 0` to `1` on MCP read surfaces. Unbounded list semantics are not safe for an LLM context server.
5. Keep the global hard cap at 12,000 estimated tokens. Tool-specific caps may be smaller, but not larger.
6. Record only counts and token totals in `ContextMetrics`.

## Risks / Trade-offs

- Default compact search changes the response shape for clients that omitted `output_mode`. Mitigation: document the breaking change and keep explicit `output_mode: "default"` available with caps.
- Final response trimming may omit data a caller expected. Mitigation: expose `context_accounting.truncation_applied`, omitted counts, and suggested next tools or targets.
- Token estimates are approximate. Mitigation: use the same deterministic estimator throughout the MCP surface.
