## Context

The MCP server (`crates/synrepo-mcp/`) ships task-shaped tools that cover most agent workflows. The CLI already has raw graph primitives (`synrepo node <id>`, `synrepo graph query "outbound <id>"`) that exercise the `GraphStore` trait directly. The MCP surface is missing these escape hatches: agents that need raw node lookup, edge traversal, graph query, overlay inspection, or provenance audit have no path forward when the card surface does not answer their question.

The existing `main.rs` in the MCP crate is 775 lines. Adding five new tool handlers without splitting will exceed the 400-line soft cap by a wide margin.

## Goals / Non-Goals

**Goals:**
- Ship five MCP primitives: `synrepo_node`, `synrepo_edges`, `synrepo_query`, `synrepo_overlay`, `synrepo_provenance`.
- Reuse the existing `GraphStore` and `OverlayStore` trait methods as the backend.
- Split the MCP main.rs to keep files under 400 lines.
- Each primitive runs under a read snapshot, matching the invariant already used by card tools.

**Non-Goals:**
- No new graph query DSL. `synrepo_query` reuses the existing CLI query syntax (`outbound <id> [edge_kind]`, `inbound <id> [edge_kind]`).
- No mutation through primitives. All five tools are read-only.
- No budget or freshness protocol on primitives. They return raw stored data without card-level framing.
- No streaming or pagination. Results are bounded by the graph size, which is inherently bounded by the repo.

## Decisions

### D1: Split MCP handlers into a tools/ sub-module

Move existing tool handlers out of `main.rs` into a `tools/` directory within the MCP crate. `main.rs` retains `SynrepoServer` struct definition, `ServerHandler` impl, `main()`, and shared helpers. Each logical group of tools gets its own file.

Alternatives considered:
- One `primitives.rs` file for just the new tools. Rejected because `main.rs` is already at 775 lines and the existing tools also need a home.
- One file per tool. Rejected as over-splitting for ~5-30 lines per handler.

Approach: `tools/mod.rs` (re-exports), `tools/cards.rs` (card/entrypoint/module/public-api/minimum-context handlers), `tools/search.rs` (search/overview/where-to-edit/change-impact), `tools/audit.rs` (findings/recent-activity), `tools/primitives.rs` (node/edges/query/overlay/provenance).

### D2: NodeId parsing in primitives

`NodeId` already implements `FromStr` (in `src/core/ids.rs`), and the CLI uses display-format strings like `file_0000000000000042`. The MCP primitives accept `id` as a string parameter and parse via `FromStr`. Invalid IDs return a structured error message listing valid formats.

### D3: synrepo_query reuses CLI query syntax

The CLI `graph query` command parses `"outbound <id> [edge_kind]"` and `"inbound <id> [edge_kind]"` in `src/bin/cli_support/graph.rs`. Extract `parse_graph_query` and `GraphQuery`/`RenderedEdge` into a shared location (e.g., `src/surface/query.rs`) so both CLI and MCP can call it. Alternatively, inline the small parse in the MCP handler since it is ~25 lines.

Decision: inline in the MCP handler. The parse logic is small and stable. Extracting it adds a cross-crate dependency for minimal gain.

### D4: synrepo_provenance returns node + edges provenance

Every graph node carries a `Provenance` field. Every edge carries one too. `synrepo_provenance(id)` returns:
1. The node's provenance (source, created_by, source_ref).
2. For each incident edge (both inbound and outbound), the edge's provenance plus the peer node ID.

This gives agents a full audit trail without needing to call `synrepo_node` separately.

### D5: synrepo_overlay returns overlay data for a node

Calls `overlay.links_for(node_id)` and `overlay.commentary_for(node_id)`. Returns commentary (if present) plus all proposed links with their status and confidence tier. Returns an explicit `{"overlay": null}` when no overlay data exists for the node, distinguishing "no overlay" from "node not found" (the latter is an error).

## Risks / Trade-offs

- **[775-line main.rs split]** The split touches every existing tool handler. Risk: merge conflict with in-flight work. Mitigation: do the split as a single self-contained task before adding any new tools.
- **[No auth on primitives]** Raw graph data is exposed to any connected agent. This matches the existing MCP security model (stdio, local-only, trust the agent). No change to threat model.
- **[No pagination]** A node with thousands of edges will return all of them. In practice, most nodes have <100 edges. Mitigation: document this as a known limit; add pagination later if real-world usage demands it.
