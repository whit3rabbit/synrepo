## Why

`synrepo_search` is already backed by the syntext lexical substrate, but the MCP surface exposes only a query string and limit. Agents still need to fall back to shell grep/ripgrep for common exact-search workflows such as path scoping, extension filtering, and case-insensitive lookup.

## What Changes

- Extend the existing MCP `synrepo_search` tool with optional grep-style filters while preserving the current `query` plus `limit` call shape.
- Return explicit metadata that identifies syntext as the engine and the substrate index as the source store.
- Keep MCP search read-only and freshness-explicit: search never triggers reconcile, watch startup, or automatic index mutation.
- Document the expanded tool contract in MCP docs.

## Capabilities

### New Capabilities

### Modified Capabilities
- `mcp-surface`: Extend `synrepo_search` from a basic lexical fallback into a filtered exact-search MCP surface.

## Impact

- Affects MCP parameter and response contracts for `synrepo_search`.
- Reuses existing `syntext::SearchOptions`; no dependency or storage schema changes.
- Adds handler-level tests for MCP search filters and backward compatibility.
