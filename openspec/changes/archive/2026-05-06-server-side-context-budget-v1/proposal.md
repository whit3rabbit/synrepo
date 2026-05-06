## Why

MCP context control must be enforced by the server, not left to agent behavior. The prior RTK-style compact-output change made compact reads available, but broad or raw tool calls can still flood the context window when a client asks badly.

## What Changes

- **BREAKING**: `synrepo_search` defaults to compact output, a limit of 10, and a small token budget.
- Add global response clamps so read tools cannot return unbounded JSON.
- Tighten search, card batch, context-pack, notes, findings, and graph primitive defaults and maximums.
- Return omitted counts, truncation metadata, suggested next handles, and accounting instead of broad raw payloads.
- Extend context metrics with response-size and flood-truncation counters.
- Document the context budget contract in the agent skill and MCP docs.

## Capabilities

### New Capabilities
- `context-budget-enforcement`: Server-side response caps, truncation metadata, and context-flood observability for MCP responses.

### Modified Capabilities
- `mcp-surface`: Default MCP context surfaces become compact and bounded, with explicit raw/deep escalation limits.
- `context-accounting`: Metrics include response clamp, deep-card, context-pack, and per-tool token totals.

## Impact

- Affected runtime paths: MCP dispatch, search, cards, context pack, graph primitives, docs search, notes, findings, resources, and metrics.
- Affected docs: `skill/SKILL.md`, `docs/MCP.md`, and `src/surface/mcp/README.md`.
- No storage migration or new dependency.
