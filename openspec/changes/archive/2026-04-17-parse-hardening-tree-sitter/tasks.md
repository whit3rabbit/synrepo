## 1. Query validation test suite

- [x] 1.1 Add a `language::supported()` iterator (or equivalent exhaustive-match helper) over every `Language` variant so validation tests cannot silently skip a new variant
- [x] 1.2 Add a test that compiles `definition_query()` for every supported `Language` and asserts the `item` and `name` captures are present
- [x] 1.3 Add a test that compiles `call_query()` for every supported `Language` and asserts the `callee` capture is present where call extraction is wired
- [x] 1.4 Add a test that compiles `import_query()` for every supported `Language` and asserts the `import_ref` capture is present where import extraction is wired
- [x] 1.5 Ensure diagnostics on failure identify the language and the query role (definition/call/import)

## 2. Pin pattern-index to SymbolKind mapping

- [x] 2.1 Add a per-language test that locks the pattern-index → `SymbolKind` mapping used by `kind_for_pattern()` against the compiled query's actual pattern ordering
- [x] 2.2 Assert every pattern index emitted by the definition query has an explicit assignment — no reliance on the `SymbolKind::Function` fallback to pass
- [x] 2.3 Decide whether to add a `debug_assert!` inside `kind_for_pattern()` (default: no) and document the decision inline if added
- [x] 2.4 Keep runtime fallback behavior unchanged for release builds

## 3. Per-language parser fixtures

- [x] 3.1 Add Rust fixtures covering functions, structs, enums, traits, modules, const/static items, and methods inside impl blocks
- [x] 3.2 Add Python fixtures covering functions, classes, methods, decorators, imports, and docstrings
- [x] 3.3 Add TypeScript fixtures covering functions, classes, interfaces, type aliases, methods, and imports
- [x] 3.4 Add TSX fixtures covering components, exported functions/classes, JSX-bearing files, and imports (asserting `ParseOutput.language == Language::Tsx`)
- [x] 3.5 Add Go fixtures covering functions, methods, structs, interfaces, imports, and comments
- [x] 3.6 Add a "supported-language fixtures are complete" test that iterates `Language::supported()` and fails if any variant has no fixture registered

## 4. Qualified-name edge-case tests

- [x] 4.1 Add Rust test for `impl<T> Foo<T> { fn bar(...) {} }` — method qualname names the impl type without generics
- [x] 4.2 Add Rust test for `impl Trait for Foo { fn bar(...) {} }` — method qualname reflects the implementing type
- [x] 4.3 Add Rust test for nested modules containing same-named symbols in different scopes
- [x] 4.4 Add Python test for class methods and nested classes/functions where supported
- [x] 4.5 Add TypeScript test for class methods and an alternate class-node shape (e.g. class expression assigned to `const`)

## 5. First-class call_refs and import_refs tests

- [x] 5.1 Add per-language tests that assert expected `ParseOutput.call_refs` entries and their local call-site names
- [x] 5.2 Add per-language tests that assert expected `ParseOutput.import_refs` entries and their raw import paths
- [x] 5.3 Add a negative test per language for intentionally unsupported import forms (confirming they are absent from `import_refs`) and document the intent inline

## 6. Malformed-source behavior

- [x] 6.1 Add a test that `parse_file` on an unsupported extension returns `None`
- [x] 6.2 Add per-language tests that supported-but-malformed source returns `Some(ParseOutput)` without panic and with deterministic best-effort extraction
- [x] 6.3 Add a determinism test that invokes `parse_file` twice on identical bytes and asserts equal `ParseOutput`
- [x] 6.4 Add a test for empty input per language (zero-length source) asserting no panic and empty-or-minimal output
- [x] 6.5 Document in module-level comments which inputs return `None`, which return empty `Some(ParseOutput)`, and which return best-effort output

## 7. Stage-4 integration coverage

- [x] 7.1 Add a stage-4 test that an ambiguous call name emits `Calls` edges to every matching candidate symbol
- [x] 7.2 Add a stage-4 test that an unresolved call or import is skipped without error
- [x] 7.3 Add a stage-4 test that TypeScript/TSX relative imports resolve to target files per the current contract
- [x] 7.4 Add a stage-4 test that Python dotted imports resolve per the current contract
- [x] 7.5 Add a stage-4 test that Rust `use` paths whose last name is intentionally unsupported are skipped, with a comment flagging this as an intentional phase-1 boundary

## 8. Optional cleanup and documentation

- [x] 8.1 Evaluate whether the validation surface motivates moving embedded queries to versioned `.scm` assets; record the decision in `design.md`
- [x] 8.2 If queries stay embedded, centralize per-language query metadata in one table so supported-language additions must update a single validation surface
- [x] 8.3 Add a module-level AGENTS/design note in `src/structure/parse/` summarizing parser invariants (query compile contract, pattern-index mapping, malformed-source semantics, stage-4-facing outputs)

## 9. Validation

- [x] 9.1 Run `make check` locally and confirm fmt + clippy + tests all pass
- [x] 9.2 Run `openspec validate parse-hardening-tree-sitter` (if available) to confirm proposal/design/specs/tasks parse cleanly
- [x] 9.3 Spot-check one failure case: temporarily break a query capture name and confirm the validation test fails loudly, then restore
- [x] 9.4 Summarize shipped test counts and any parser-behavior clarifications back into `design.md` Open Questions before archive
