## Context

`synrepo_search` currently returns exact syntext rows as JSON, while card-shaped tools include token accounting and budget semantics. Agents can save tokens with cards after they know a target, but broad lexical search still emits raw snippets and lacks an MCP-native compact mode. The implementation must preserve existing default responses and the read-only search freshness contract.

## Goals / Non-Goals

**Goals:**

- Add deterministic compact output for search and search artifacts in context packs.
- Keep card accounting unchanged and treat cards as the native compact context surface.
- Persist only aggregate compact-output metrics, never query or result content.
- Keep `output_mode` opt-in so existing clients are not broken.

**Non-Goals:**

- No arbitrary command runner or shell-output proxy in MCP.
- No LLM summarization, explain refresh, or commentary generation for compact output.
- No graph, overlay, or SQLite schema migration.

## Decisions

1. Use an opt-in `output_mode` parameter rather than new tools. This keeps the MCP surface small and preserves existing tool names. New tools were rejected because they would duplicate search and context-pack behavior.
2. Add `output_accounting` as a sibling to existing response fields, not a replacement for `context_accounting`. `context_accounting` remains card-specific, while `output_accounting` describes response compaction.
3. Group compact search by file path with line previews and `suggested_card_targets`. This preserves routing utility while giving agents an immediate escalation path to `synrepo_card` or `synrepo_context_pack`.
4. Estimate tokens deterministically from serialized JSON byte length using the existing four-bytes-per-token approximation. This matches existing context accounting without reading extra source bodies.
5. Store only aggregate compact counters in `ContextMetrics` with `#[serde(default)]`. This keeps older metrics files readable and maintains the existing privacy boundary.

## Risks / Trade-offs

- Compact previews may omit a line the agent would have used directly. Mitigation: return omitted counts, preserve rank order, and provide card targets for escalation.
- Token estimates are approximate. Mitigation: label them as estimates and use the same deterministic method already used elsewhere.
- Adding metrics fields affects JSON/Prometheus consumers. Mitigation: append fields with serde defaults and update tests/docs.
- Context-pack search artifacts could diverge from direct search. Mitigation: share compacting code between direct search and context-pack search artifacts.
