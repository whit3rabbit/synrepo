## Why

Agents working on a symbol or file need just enough surrounding context to act safely: what calls it, what it calls, what decisions govern it, and what files tend to change alongside it. Today, agents must issue multiple MCP calls (`synrepo_card`, `synrepo_change_impact`, `synrepo_where_to_edit`) and manually assemble a neighborhood view. This is fragile, expensive in tokens, and easy to get wrong. A single `synrepo_minimum_context` tool that returns a budget-scoped minimum-useful-neighborhood directly implements invariant 4 ("smallest truthful context first") as a first-class MCP operation.

## What Changes

- Add `synrepo_minimum_context` MCP tool that accepts a focal symbol or file (by node ID or qualified path) and a budget tier (`tiny`, `normal`, `deep`).
- Return the focal card plus a 1-hop neighborhood: outbound `Calls`/`Imports` edges (as summary cards), incoming `Governs` edges (as `DecisionCard` summaries), and top-N co-change partners (from `GitHistoryInsights.co_changes`, already computed by `git-data-surfacing-v1`).
- Neighborhood size is bounded by budget tier: `tiny` returns only the focal card with link counts, `normal` adds 1-hop structural edges and top-3 co-change partners, `deep` adds full neighbor cards and top-5 co-change partners.
- The tool reads only from the graph store (no overlay content). Co-change data comes from the per-file git-intelligence cache already wired into the card compiler.

## Capabilities

### New Capabilities

- `minimum-context`: Defines the `synrepo_minimum_context` MCP tool contract, neighborhood composition rules, budget-tier escalation behavior, and response shape.

### Modified Capabilities

- `mcp-surface`: Extends the tool registry to include `synrepo_minimum_context` alongside the existing task-first tools. Adds a requirement that the minimum-context tool returns budget-bounded neighborhoods with provenance labels.

## Impact

- **Code**: New MCP tool handler in `crates/synrepo-mcp/`, new neighborhood resolution logic in `src/surface/card/` (likely a `neighborhood.rs` module or extension of the existing compiler).
- **Dependencies**: Reuses existing `GraphCardCompiler`, `GraphStore` read snapshots, and the git-intelligence cache. No new external dependencies.
- **Specs**: `mcp-surface` spec gains one new tool definition; new `minimum-context` spec defines the neighborhood contract.
