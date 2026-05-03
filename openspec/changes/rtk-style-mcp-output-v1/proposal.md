## Why

Agents already use synrepo cards to avoid expensive cold source reads, but lexical MCP search still returns raw match rows. That makes broad searches noisy and pushes agents back toward shell-output filtering even when the MCP server has enough context to compact results safely.

## What Changes

- Add an opt-in compact output mode for safe MCP read surfaces.
- Preserve default `synrepo_search` and `synrepo_context_pack` responses for existing clients.
- Return deterministic compact search groups with accounting metadata, omitted counts, and suggested card targets.
- Extend context metrics with content-free compact-output counters.
- Document cards as synrepo's native RTK-style context format, and keep arbitrary command execution out of MCP.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `mcp-surface`: Add opt-in compact output parameters and response contracts for search and context packs.
- `context-accounting`: Add compact-output accounting and content-free operational counters.

## Impact

- Affected runtime paths: MCP search, context pack assembly, shared MCP helpers, and context metrics.
- Affected documentation: MCP docs and agent-facing skill guidance.
- No storage schema migration, no new dependencies, and no MCP command execution surface.
