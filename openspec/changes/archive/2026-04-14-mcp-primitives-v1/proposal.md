## Why

The MCP surface ships task-shaped tools (cards, entrypoints, search, overview) that serve most agent workflows well. But FOUNDATION.md and FOUNDATION-SPEC.md both define low-level primitives (`synrepo_node`, `synrepo_edges`, `synrepo_query`, `synrepo_overlay`) that are not yet implemented. These are the escape hatches: when no task-shaped tool answers the agent's question, the primitives let a power-user agent inspect and traverse the raw graph and overlay directly. Without them, any question that falls outside the card surface is a dead end.

## What Changes

- Add `synrepo_node(id)` MCP tool: raw node lookup by `NodeId`, returns full stored metadata as JSON.
- Add `synrepo_edges(id, direction?, edge_types?)` MCP tool: edge traversal from a node, optional direction and type filters.
- Add `synrepo_query(query)` MCP tool: structured graph query, reusing the existing `graph query` DSL from the CLI.
- Add `synrepo_overlay(node_id)` MCP tool: overlay entry lookup for a given node, returns commentary, findings, and proposed links if present.
- Add `synrepo_provenance(node_id)` MCP tool: provenance audit for a given node, returns the full provenance chain (source, created_by, source_ref) for the node and its incident edges.

No breaking changes to existing tools.

## Capabilities

### New Capabilities

None. All five tools are additions to the existing `mcp-surface` capability.

### Modified Capabilities

- `mcp-surface`: Adds five low-level primitive tool requirements to the existing MCP surface spec. The task-shaped tools are unchanged; the primitives section goes from "planned" to "specified."

## Impact

- `crates/synrepo-mcp/src/main.rs`: five new handler methods, five new param structs. File is at 775 lines; this will exceed the 400-line soft cap and require splitting into a sub-module (e.g., `primitives.rs` or a `tools/` directory).
- `src/surface/` or `src/store/`: query execution path may need a thin adapter between the MCP handler and the existing `GraphStore` / `OverlayStore` traits.
- `openspec/specs/mcp-surface/spec.md`: new requirements for each primitive tool with scenarios.
- No new dependencies. All primitives delegate to existing store trait methods.
