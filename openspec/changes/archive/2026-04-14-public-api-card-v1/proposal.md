## Why

Agents navigating a codebase need to understand what a module or crate exposes at its boundary: which symbols are public, which are entry points, and which public APIs changed recently. The current card surface has `ModuleCard` (what files live in this directory?) and `EntryPointCard` (where does execution start?) but neither answers "what is the public API of this module?" with visibility filtering and API-change history.

All required data is already in the graph: `SymbolNode.signature` contains the full declaration prefix (including `pub`), git intelligence is wired end-to-end from `git-data-surfacing-v1`, and entry point classification exists in `entry_point::classify_kind`. No new infrastructure is required.

## What Changes

- Add `PublicAPIEntry` type: a single exported symbol entry with id, name, kind, signature, location, and optional `last_change`.
- Add `PublicAPICard` type: aggregates public symbols, public entry points, and (at deep budget) recent API changes for a directory scope.
- Add `public_api_card` method to `CardCompiler` trait.
- Add `GraphCardCompiler::public_api_card` implementation in `src/surface/card/compiler/public_api.rs`.
- Add `synrepo_public_api` MCP tool that compiles and returns a `PublicAPICard` for a given path.
- Register the new card type and tool in the canonical specs.

## Capabilities

### New Capabilities

- `public-api-card`: `PublicAPICard` contract, visibility filtering rules, budget-tier truncation, recency window definition, and `synrepo_public_api` MCP tool definition.

### Modified Capabilities

- `cards`: Add `PublicAPICard` and `PublicAPIEntry` to the card-type taxonomy.
- `mcp-surface`: Register `synrepo_public_api` tool.

## Impact

- `src/surface/card/types.rs` — new `PublicAPIEntry` and `PublicAPICard` structs.
- `src/surface/card/mod.rs` — re-export + `public_api_card` trait method.
- `src/surface/card/compiler/public_api.rs` — new file, full impl.
- `src/surface/card/compiler/mod.rs` — `mod public_api` declaration + trait dispatch.
- `src/surface/card/compiler/entry_point/mod.rs` — `classify_kind` visibility: `fn` → `pub(super)`.
- `crates/synrepo-mcp/src/main.rs` — `PublicAPICardParams` struct + `synrepo_public_api` handler.
- `openspec/specs/cards/spec.md` — updated card-type taxonomy.
- `openspec/specs/mcp-surface/spec.md` — updated tool registry.
- `skill/SKILL.md` — new tool row.
- No new dependencies. No overlay or LLM involvement.

## Limitations (v1)

Visibility inference uses `signature.starts_with("pub")`, which is Rust-specific. Python, TypeScript, and Go modules will return empty `public_symbols` lists. A `visibility` field on `SymbolNode` would generalize this; deferred to a later change.
