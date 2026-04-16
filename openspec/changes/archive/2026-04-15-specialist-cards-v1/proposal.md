## Why

Agents navigating an unfamiliar codebase need to understand two things that the current card surface does not expose: *how to reach a symbol* (the call chain from an entry point) and *what tests cover a given scope*. Both are derivable from the existing graph (parser-observed `Calls` and `Imports` edges, plus `SymbolKind::Test` markers) with no new infrastructure. Shipping them removes a gap the roadmap identifies as Phase 3 and the natural next cards to build after `EntryPointCard` and `ModuleCard`.

## What Changes

- Add `CallPathCard` type: a graph-derived card that traces execution paths from entry points to a target symbol using `Calls` and `Imports` edges. Answers "how do I reach this function?"
- Add `TestSurfaceCard` type: a graph-derived card that discovers test functions related to a scope (file or symbol). Answers "what tests cover this code?"
- Add `synrepo_call_path` MCP tool that compiles and returns a `CallPathCard` for a given symbol.
- Add `synrepo_test_surface` MCP tool that compiles and returns a `TestSurfaceCard` for a given scope.
- Wire both cards into `GraphCardCompiler` following the existing `EntryPointCard` / `ModuleCard` pattern (graph-only, no overlay, budget-tier truncation).
- Register both new card types in the canonical cards spec.

## Capabilities

### New Capabilities

- `call-path-card`: CallPathCard contract, path-traversal rules, budget-tier truncation, and `synrepo_call_path` MCP tool definition.
- `test-surface-card`: TestSurfaceCard contract, test-discovery heuristics, budget-tier truncation, and `synrepo_test_surface` MCP tool definition.

### Modified Capabilities

- `cards`: Add `CallPathCard` and `TestSurfaceCard` to the card-type taxonomy in the canonical cards spec.
- `mcp-surface`: Register `synrepo_call_path` and `synrepo_test_surface` MCP tools.

## Impact

- `src/surface/card/types.rs` — new `CallPathCard` and `TestSurfaceCard` structs, `CallPathEdge` and `TestEntry` payload types.
- `src/surface/card/compiler/` — new compile methods in `GraphCardCompiler` for both cards; path-traversal logic for `CallPathCard` (BFS/DFS over `Calls` edges, bounded depth).
- `crates/synrepo-mcp/src/main.rs` — two new MCP tool handlers.
- `openspec/specs/cards/spec.md` — updated card-type taxonomy.
- `openspec/specs/mcp-surface/spec.md` — updated tool registry.
- No new dependencies. No overlay or LLM involvement.
