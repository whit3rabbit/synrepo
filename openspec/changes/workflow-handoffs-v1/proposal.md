## Why

Developers often need to hand off work between Claude Code sessions or to other team members. synrepo already collects repair recommendations, cross-link candidates, and git hotspots but lacks a unified surface to present actionable next steps. A dedicated handoffs surface aggregates these signals into a readable, prioritized list.

## What Changes

- Add `synrepo handoffs` CLI command that reads repair report, overlay candidates, and git hotspots, then emits prioritized actionable items
- Add `synrepo_next_actions` MCP tool providing on-demand handoffs surface to AI agents
- No new mutable store: reads existing graph, overlay, and repair-log data only
- Handoffs are derived at query time, not persisted

## Capabilities

### New Capabilities

- `handoffs-surface`: Derived surface aggregating repair recommendations, pending cross-link candidates, and git hotspot signals into prioritized actionable items. Read-only, computed per-query.

### Modified Capabilities

- `mcp-surface`: Add `synrepo_next_actions` tool to the MCP surface spec
- `cards-and-mcp-v1` (via exports-and-views): Add `synrepo handoffs` command to CLI surface spec

## Impact

- New CLI command: `synrepo handoffs [--json] [--limit N]`
- New MCP tool: `synrepo_next_actions`
- No new dependencies
- Reads from: repair report (`repair-log.jsonl`), overlay candidates, git hotspot data
