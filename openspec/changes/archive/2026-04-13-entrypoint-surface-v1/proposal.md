## Why

`ModuleCard` exists as a struct placeholder and `EntryPointCard` does not exist at all, leaving agents with no structured way to answer "what does this directory do?" or "where does execution start?" — two of the most common orientation questions on an unfamiliar codebase. `synrepo_entrypoints` is listed in the spec and skill as a planned tool but is absent from the MCP surface.

## What Changes

- Implement `GraphCardCompiler::module_card()` — compile a real `ModuleCard` from graph-derived directory facts (file list, public symbols, submodule structure).
- Add `EntryPointCard` struct and `GraphCardCompiler::entry_point_card()` — heuristic detection of execution roots (binary `main`, CLI command handlers, HTTP route handlers, public library roots) using graph-observed symbol names and file paths, with no LLM involvement.
- Wire `synrepo_entrypoints(scope?, budget?)` as a new MCP tool returning a bounded list of `EntryPointCard`s for the requested scope.
- Update `skill/SKILL.md` to reflect the new tool in the current MCP surface.

## Capabilities

### New Capabilities

- `module-card`: Compiled `ModuleCard` contract — fields, budget-tier truncation rules, and source labeling for directory-scoped context packets.
- `entry-point-card`: `EntryPointCard` struct, heuristic detection strategy, budget-tier behavior, and the `EntryPointKind` taxonomy (binary, cli_command, http_handler, lib_root).

### Modified Capabilities

- `mcp-surface`: Add the `synrepo_entrypoints(scope?, budget?)` tool contract, including parameter schema, response shape, and error behavior when no entry points are found.

## Impact

- `src/surface/card/types.rs` — add `EntryPointCard`, `EntryPoint`, `EntryPointKind` structs.
- `src/surface/card/compiler/` — add `module_card()` and `entry_point_card()` methods; implement entry-point heuristic detection.
- `src/surface/card/mod.rs` — re-export new types; extend `CardCompiler` trait.
- `crates/synrepo-mcp/src/main.rs` — add `synrepo_entrypoints` tool and `EntrypointsParams`.
- `skill/SKILL.md` — update tool count and surface listing.
- `openspec/specs/cards/spec.md` — no change (new capabilities handled by dedicated delta specs).
- No new dependencies required; detection is graph-only and heuristic-based.
