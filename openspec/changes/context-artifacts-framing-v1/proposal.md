## Why

Synrepo already has deterministic graph facts, cards, context packs, budgets, and MCP delivery, but the product language still collapses those layers into "cards." A sharper framing makes the trust boundary and agent-facing value easier to explain without changing runtime behavior.

## What Changes

- Define the product model as `repo files -> graph facts -> code artifacts -> task contexts -> cards/MCP`.
- Introduce `context-artifacts` as the durable concept for compiled, task-usable records and bundles.
- Reframe cards as the serialized delivery packet for artifacts and task contexts, not the only product abstraction.
- Update docs and agent-facing guidance to use the new vocabulary while preserving the existing workflow and MCP surface.
- Defer `synrepo_ask`, new Rust context modules, storage, schema changes, artifact caches, and field-level citation wrappers.

## Capabilities

### New Capabilities

- `context-artifacts`: Defines graph facts, code artifacts, task contexts, and cards as distinct product concepts.

### Modified Capabilities

- `foundation`: Refine the product wedge from card-only language to the layered context artifact model.
- `cards`: Clarify cards as delivery packets for compiled artifacts and contexts while preserving existing card contracts.
- `mcp-surface`: Clarify `synrepo_context_pack` as the current batched card/context delivery surface without adding tools or changing schemas.
- `agent-doctrine`: Update terminology in agent-facing guidance without changing workflow rules.

## Impact

- Affected docs: `README.md`, `docs/FOUNDATION.md`, `docs/ARCHITECTURE.md`, `docs/MCP.md`, and `skill/SKILL.md`.
- Affected source prose: `src/surface/agent_doctrine.rs`.
- Affected OpenSpec artifacts: new `context-artifacts` capability and deltas for foundation, cards, MCP surface, and agent doctrine.
- No runtime code path, storage schema, MCP tool, request or response shape, dependency, or migration changes.
