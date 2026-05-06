## Why

Phase 1 named the layered context-artifact model but intentionally avoided runtime changes. Agents still need to choose among several low-level MCP tools before they receive a task-shaped packet, which can waste tool calls and flood context when a broad task could be compiled once.

## What Changes

- Add `src/surface/context/` as a deterministic task-context planning layer.
- Add `synrepo_ask` as the read-only MCP front door for plain-language task packets.
- Compile asks into existing `synrepo_context_pack` targets rather than adding new storage or changing card schemas.
- Return a compact envelope with `answer`, `cards_used`, `evidence`, `grounding`, `omitted_context_notes`, `next_best_tools`, and `context_packet`.
- Update agent guidance and MCP docs so `synrepo_ask` is the default broad-task path and lower-level tools remain drill-downs.

## Capabilities

### New Capabilities

- `context-artifacts`: Adds deterministic task-context recipes and request/grounding/budget types for runtime packet compilation.

### Modified Capabilities

- `mcp-surface`: Adds the `synrepo_ask` read-only MCP tool and schema.
- `agent-doctrine`: Makes `synrepo_ask` the default broad task-context front door while preserving existing bounded escalation.

## Impact

- Affected runtime paths: `src/surface/context/`, `src/surface/mcp/ask.rs`, and MCP tool registration.
- Affected docs: `README.md`, `docs/MCP.md`, `src/surface/mcp/README.md`, and `skill/SKILL.md`.
- Affected tests: MCP registration/schema tests plus context/ask unit tests.
- No storage migration, artifact cache, persistent registry, new dependency, source edit capability, or overlay promotion.
