## Why

The structural parser is load-bearing: it feeds symbol extraction and stage-4 cross-file resolution, yet today it can degrade silently when a tree-sitter grammar or embedded query drifts. Query-compile failures cache as `None`, `kind_for_pattern()` falls back to `Function`, and stage 4 skips unresolved names without error, so parser regressions show up as quiet graph-quality loss rather than a CI failure.

## What Changes

- Treat tree-sitter query compilation as a hard CI contract: every supported language's `definition_query`, `call_query`, and `import_query` must compile and expose required captures in tests.
- Pin the pattern-index to `SymbolKind` mapping per language via tests so ordering drift fails loudly.
- Expand parser fixture coverage to all supported languages (Rust, Python, TypeScript, **TSX**, Go) for symbols, qualified names, `call_refs`, `import_refs`, doc comments, and syntax-error behavior.
- Add targeted qualified-name edge-case tests (Rust generic/trait impls, nested modules, Python/TS nested classes).
- Treat `ParseOutput.call_refs` and `ParseOutput.import_refs` as first-class tested outputs, not incidental data.
- Pin malformed-source behavior: unsupported extension → `None`; supported-but-malformed → deterministic best-effort `Some(ParseOutput)`.
- Add stage-4 integration tests that lock in the current approximate-resolution contract (ambiguous-name fanout, TS relative imports, Python dotted imports, Rust `use` skipping, unresolved-skip).
- Out of scope: changing the stage-4 resolution contract, replacing tree-sitter, moving queries to external `.scm` assets (evaluated but not required).

## Capabilities

### New Capabilities

- `structural-parse`: Tree-sitter-backed source parsing that produces deterministic `ParseOutput` (symbols, qualified names, signatures, doc comments, `call_refs`, `import_refs`) across the supported language set, with explicit query-validation, kind-mapping, and malformed-source contracts that CI enforces.

### Modified Capabilities

<!-- None. Stage-4 behavior is tested as an integration surface but its contract is not changing. -->

## Impact

- Affected code:
  - `src/structure/parse/language.rs` (query definitions, capture contracts)
  - `src/structure/parse/extract/mod.rs` (query cache init, `kind_for_pattern`, `parse_file`)
  - `src/structure/parse/extract/qualname.rs` (qualified-name edge cases)
  - `src/structure/parse/tests.rs` (expanded per-language coverage, TSX)
  - `src/pipeline/structural/stage4.rs` and its tests (integration coverage only)
- Affected contracts: parser produces `ParseOutput` whose query compilation, capture presence, kind mapping, and reference extraction are testable invariants rather than runtime best-effort.
- Dependencies: no new runtime deps; test-only fixtures under `src/structure/parse/` (or a sibling fixtures module).
- Runtime risk: none intended — runtime stays permissive so ordinary malformed user source is not escalated to a fatal error; strictness is scoped to tests/CI.
- Review surface: one focused change rather than folding into broader structural work, because the load-bearing risk is concentrated in the parser and its test surface.
