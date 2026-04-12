## Why

`SymbolCard` is the primary card type agents use to orient in a codebase, but `signature` and `doc_comment` — the two fields most useful for that purpose — are always `None`. The fields exist in `ExtractedSymbol`, flow through `SymbolNode`, and are read by `GraphCardCompiler`, but tree-sitter extraction logic for them was never implemented. Agents currently get a card that identifies a function by name and kind but cannot show what it does or how to call it.

## What Changes

- Add `doc_comment_query()` and `signature_query()` methods to `Language` in `src/structure/parse/language.rs` for Rust, Python, and TypeScript/TSX.
- Extend `parse_file()` in `src/structure/parse/extract.rs` to run these queries and populate `ExtractedSymbol.signature` and `ExtractedSymbol.doc_comment`.
- Add insta snapshot tests for `SymbolCard` at all three budget tiers (tiny, normal, deep) in `src/surface/card/`.
- Split `src/surface/card/compiler.rs` (currently 420 lines, over the 400-line limit) into focused sub-modules before adding test fixtures.

## Capabilities

### New Capabilities

None. This change implements behavior that is implicitly required by the existing card and graph contracts but was deferred.

### Modified Capabilities

- `graph`: Add requirements for what `SymbolNode.signature` and `SymbolNode.doc_comment` must contain after structural parse. Currently the graph spec describes symbol identity and kind but does not require that signature or doc comment fields be populated.

## Impact

- `src/structure/parse/language.rs` — new query methods per language
- `src/structure/parse/extract.rs` — extraction logic, lines 108–109
- `src/surface/card/compiler.rs` — split into sub-modules; no behavior change
- `src/surface/card/` — new snapshot tests
- No API or MCP surface changes; card fields are already defined and wired
- No breaking changes; fields go from `None` to populated
