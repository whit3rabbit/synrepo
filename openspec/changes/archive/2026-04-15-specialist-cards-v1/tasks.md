## 1. CallPathCard types

- [x] 1.1 Define `CallPathEdge` and `CallPathCard` structs in `src/surface/card/types.rs` (from/to symbol refs, edge_kind, truncated flag, paths list, paths_omitted count)
- [x] 1.2 Verify `cargo check` passes with the new types

## 2. CallPathCard compiler

- [x] 2.1 Add `compile_call_path_card` method to `GraphCardCompiler` in `src/surface/card/compiler/mod.rs`
- [x] 2.2 Implement backward BFS over `Calls` edges: query inbound `Calls` edges for target symbol, walk predecessors up to depth 8, collect paths that terminate at entry point symbols
- [x] 2.3 Implement path deduplication: at most 3 paths per (entry_point, target) pair, count omitted in `paths_omitted`
- [x] 2.4 Implement truncation for paths exceeding depth budget: mark final edge `truncated: true`
- [x] 2.5 Implement budget-tier truncation: tiny (entry + target names + hop count only), normal (full edge list), deep (signatures + file locations + depth 12)
- [x] 2.6 Write unit tests for backward BFS, dedup, truncation, and empty-result cases

## 3. TestSurfaceCard types

- [x] 3.1 Define `TestEntry` and `TestSurfaceCard` structs in `src/surface/card/types.rs` (symbol_id, qualified_name, file_path, source_file, association enum)
- [x] 3.2 Verify `cargo check` passes with the new types

## 4. TestSurfaceCard compiler

- [x] 4.1 Add `compile_test_surface_card` method to `GraphCardCompiler` in `src/surface/card/compiler/mod.rs`
- [x] 4.2 Implement file-path association heuristics: sibling test file patterns (`*_test.rs`, `test_*.py`, `*.test.ts`, `*.spec.ts`), parallel test directory (`tests/<stem>`), nested test module (`tests/`, `__tests__/`)
- [x] 4.3 Implement symbol-kind filter: include symbols with `SymbolKind::Test` from associated test files
- [x] 4.4 Compute `association` field: `"symbol_kind"`, `"path_convention"`, or `"both"` based on which signals matched
- [x] 4.5 Implement budget-tier truncation: tiny (counts only), normal (entries without signatures), deep (signatures + doc comments + `covers` field from `Calls` edges)
- [x] 4.6 Write unit tests for path heuristics, symbol-kind filtering, association computation, and empty-result cases

## 5. MCP tool wiring

- [x] 5.1 Add `synrepo_call_path` MCP tool handler in `crates/synrepo-mcp/src/main.rs`: accepts target symbol ID or qualified name, optional budget parameter (default normal), returns `CallPathCard` JSON
- [x] 5.2 Add `synrepo_test_surface` MCP tool handler in `crates/synrepo-mcp/src/main.rs`: accepts scope (file path or directory), optional budget parameter (default normal), returns `TestSurfaceCard` JSON
- [x] 5.3 Verify both tools appear in MCP capabilities output

## 6. Spec and doc updates

- [x] 6.1 Update `openspec/specs/cards/spec.md`: add `CallPathCard` and `TestSurfaceCard` to the card-type taxonomy
- [x] 6.2 Update `openspec/specs/mcp-surface/spec.md`: add `synrepo_call_path` and `synrepo_test_surface` to the tool list
- [x] 6.3 Verify `openspec validate` passes for both updated specs

## 7. Integration verification

- [x] 7.1 Run `make check` (fmt, clippy, tests) and confirm clean pass
- [x] 7.2 Run `cargo run -- export --format json --deep` and confirm `CallPathCard` and `TestSurfaceCard` fields serialize without error
- [x] 7.3 Update ROADMAP.md: move `CallPathCard` and `TestSurfaceCard` from "not yet implemented" to shipped surface, update Phase 3 status