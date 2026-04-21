# Adding a new language

Structural parsing currently supports Rust, Python, TypeScript, TSX, and Go. Adding a new language is surface-enforced: validation and fixture tests compile-break or fail loud if any required update is missed. Parser invariants are documented in the `src/structure/parse/mod.rs` module doc.

## Files you MUST touch

1. **`Cargo.toml`** — add the grammar crate: `tree-sitter-<lang> = "<version>"`. Follow the version style of existing entries.
2. **`src/structure/parse/language.rs`** — the single source of truth for per-language metadata. Update, in order:
   - `pub enum Language { … }` — add the variant.
   - `Language::supported()` — append the variant to the slice. Validation and fixture tests iterate this.
   - `Language::display_name()` — lowercase label used in diagnostics (`"rust"`, `"python"`, …).
   - `Language::from_extension()` — map the file extension(s) to the variant.
   - `Language::tree_sitter_language()` — wire `tree_sitter_<lang>::LANGUAGE.into()`.
   - `Language::definition_query()` — return a `&'static str` holding the tree-sitter query. Must expose `@item` (the node) and `@name` (the identifier) captures. Add a `const <LANG>_DEFINITION_QUERY: &str = r#" … "#;` above the match.
   - `Language::call_query()` — must expose a `@callee` capture. Add `const <LANG>_CALL_QUERY` the same way.
   - `Language::import_query()` — must expose an `@import_ref` capture. Add `const <LANG>_IMPORT_QUERY` the same way.
   - `Language::kind_map()` — return a `&'static [SymbolKind]` whose length equals the definition query's `pattern_count()`. Each index maps a query pattern to a `SymbolKind`. Add `const <LANG>_KIND_MAP` above and include a comment block enumerating which pattern index maps to which kind.
   - `Language::call_mode_map()` — return a `&'static [CallMode]` whose length equals the call query's `pattern_count()`. Each index maps a call-query pattern to `CallMode::Free` (bare call) or `CallMode::Method` (receiver-qualified). Add `const <LANG>_CALL_MODE_MAP` alongside `<LANG>_CALL_QUERY` with a comment block enumerating the mapping.
3. **`src/structure/parse/fixture_tests.rs`** — add an entry to the `FIXTURES` table with representative source, expected `(symbol_name, SymbolKind)` pairs, and expected `import_refs`. The `fixtures_cover_every_supported_language` test will fail until this is present.
4. **`src/structure/parse/extract/visibility.rs`** — add a match arm for the new variant in `extract_visibility` to populate `Visibility::Public`, `Visibility::Private`, or `Visibility::Crate` per the language's visibility rules.

## Files you PROBABLY need to touch

5. **`src/structure/parse/extract/docs.rs`** — add `match` arms for the new variant in `extract_doc_comment` and `extract_signature` if you want doc-comment and signature extraction. Without this, the new language gets `None` for both (Go is the current example of an unwired language here).
6. **`src/pipeline/structural/stage4.rs::resolve_import_ref`** — if you want cross-file `Imports` edges resolved for this language, extend the path/extension dispatch. Without this, `import_refs` are still captured by the parser but stage 4 silently skips resolution (phase-1 boundary; Rust and Go sit here today).

## Tests you SHOULD add

7. **`src/structure/parse/validation_tests.rs`** — add the variant's kind map pin to the per-language pin test. The compile/capture-presence tests iterate `Language::supported()` automatically, so they cover the new language without edits.
8. **`src/structure/parse/qualname_tests.rs`** — add an edge-case test for the language's fragile qualname constructs (nested scopes, impl-style blocks, class expressions, etc.).
9. **`src/structure/parse/refs_tests.rs`** — add positive `call_refs`/`import_refs` tests and negative tests for intentionally unsupported forms.
10. **`src/structure/parse/malformed_tests.rs`** — add a malformed-source test and extend `empty_input_returns_some_with_no_symbols_per_language` to cover the new extension.
11. **`src/pipeline/structural/tests/edges.rs`** — if you wired stage-4 resolution in step 5, add an import-resolution contract test.

## Verification

- `cargo test --lib structure::parse::` — full parse-layer test suite.
- `cargo test --lib pipeline::structural::tests::edges::` — stage-4 integration tests.
- A broken query capture name fails `validation_tests` with a message naming the language, the query role (definition/call/import), and the missing capture — use this as your feedback loop.
