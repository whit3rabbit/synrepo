## Context

The current `synrepo_context_pack` can batch known files, symbols, directories, tests, call paths, searches, and minimum-context artifacts. It is still target-oriented: the agent must already know what to ask for. Phase 2 adds a thin deterministic compiler above it so a task-shaped request can be converted into bounded context-pack targets in one MCP call.

## Decisions

1. Keep `synrepo_ask` read-only. It may update existing best-effort MCP/context metrics through the same paths as context packs, but it must not mutate source, overlay notes, commentary, or graph facts.
2. Do not create a persistent artifact registry in this phase. The response is a task-context packet, not a cached durable artifact.
3. Use hardcoded built-in recipes first: `explain_symbol`, `trace_call`, `review_module`, `security_review`, `release_readiness`, `fix_test`, and `general`.
4. Compile recipes to existing `ContextPackTarget` kinds. This reuses card accounting, search compaction, snapshot reads, typed error artifacts, and budget caps.
5. Treat evidence extraction as best-effort over existing packet fields. Source spans are included when the underlying artifact exposes line data; otherwise the span is `null` rather than fabricated.
6. Keep overlay excluded by default. `ground.allow_overlay = true` maps to context-pack `include_notes`; advisory overlay content remains labeled and non-canonical.

## Risks / Trade-offs

- [Risk] Agents may treat `synrepo_ask.answer` as an LLM-authored review.
  Mitigation: the answer is a compact deterministic packet summary, and actual context lives in `context_packet`.
- [Risk] Hardcoded recipes are initially coarse.
  Mitigation: recipes are isolated under `src/surface/context/` and can later get eval-driven improvements without touching graph extraction.
- [Risk] Citations are not yet field-level for every card field.
  Mitigation: phase 2 reports packet-level evidence and explicit grounding status, leaving field-level `Cited<T>` for a later change.
