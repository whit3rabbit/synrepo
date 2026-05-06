## Context

Synrepo already compiles repository state into deterministic, budgeted card surfaces. The runtime shape is ahead of the product vocabulary: graph facts, card payloads, context packs, MCP resources, trust labels, and context accounting all exist, but docs and agent guidance mostly describe the end packet as "cards." That hides the layered model that makes the trust boundary strong.

## Goals / Non-Goals

**Goals:**

- Establish the vocabulary `repo files -> graph facts -> code artifacts -> task contexts -> cards/MCP`.
- Keep graph facts canonical, code artifacts compiled and deterministic, task contexts workflow-shaped, and cards/MCP as delivery packets.
- Align docs, OpenSpec, and agent-facing guidance with the same vocabulary.
- Preserve the existing default agent workflow and budget escalation doctrine.

**Non-Goals:**

- No `synrepo_ask` MCP tool.
- No new Rust modules under `src/surface/context/`.
- No artifact registry, cache, SQLite table, JSONL store, or invalidation workflow.
- No field-level citation wrapper or card response schema change.
- No MCP tool rename, request shape change, or response shape change.

## Decisions

1. Treat this as a framing change, not a runtime feature. The current `synrepo_context_pack` remains the batching surface, and cards remain the serialized format agents consume today.
2. Add a new OpenSpec capability, `context-artifacts`, to define the vocabulary once. Existing specs get small added requirements that point their current behavior at that vocabulary.
3. Update the canonical doctrine text in `src/surface/agent_doctrine.rs` only where terminology improves clarity. Workflow rules, do-not rules, and product-boundary rules stay the same so existing tests remain meaningful.
4. Avoid direct stable spec edits in this active change. The change carries delta specs and can update stable specs when archived.

## Risks / Trade-offs

- [Risk] Agents may interpret "artifact" as a new persistent object.
  Mitigation: Docs must state that Phase 1 adds no artifact registry or storage.
- [Risk] Reframing cards as delivery packets may sound like cards are less important.
  Mitigation: Docs should say cards remain the current native compact context format and MCP delivery unit.
- [Risk] Doctrine edits can drift from generated shims.
  Mitigation: Use the canonical doctrine source and run focused agent-shim tests.
