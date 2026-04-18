## 1. Baseline metrics

- [ ] 1.1 On a clean `synrepo init` of synrepo itself, record `graph.count_edges_by_kind(EdgeKind::Calls)` and the top-20 most-fan-out short names (`map`, `get`, `new`, etc.) from the current implementation.
- [ ] 1.2 Capture the same metrics on a mid-size Python (e.g., `flask` at pinned tag) and TS (e.g., `axios` at pinned tag) corpus.
- [ ] 1.3 Record baseline numbers in `openspec/changes/stage4-call-scope-narrowing-v1/notes.md` (create if needed) for before/after comparison.

## 2. Extend `ExtractedCallRef`

- [ ] 2.1 In `src/structure/parse/mod.rs:95–99`, add `pub callee_prefix: Option<String>` and `pub is_method: bool` to `ExtractedCallRef`.
- [ ] 2.2 Update any downstream consumer that constructs `ExtractedCallRef` (tests, `extract_call_refs`).

## 3. Extend per-language call queries

- [ ] 3.1 In `src/structure/parse/language.rs`, rewrite each `*_CALL_QUERY` to add:
  - A pattern for free-function calls (no prefix) with only `@callee`.
  - A pattern for method/attribute calls with `@callee_prefix` (on the receiver/value/object) and `@callee` (on the method name).
  - A pattern for scoped/qualified calls with `@callee_prefix` (on the path/type) and `@callee` (on the trailing identifier).
- [ ] 3.2 Add a `call_mode_map: &'static [CallMode]` per language, analogous to `kind_map`, to associate each query pattern index with `CallMode::Free` or `CallMode::Method`.
- [ ] 3.3 Add `Language::call_mode_map(self) -> &'static [CallMode]`.
- [ ] 3.4 Define `pub enum CallMode { Free, Method }` in `src/structure/parse/mod.rs`.

## 4. Populate new fields in the parser

- [ ] 4.1 In `src/structure/parse/extract/mod.rs::extract_call_refs`, after cache lookup:
  - Read `m.pattern_index` and look up `call_mode_map[pattern_index]` → sets `is_method`.
  - For each match, find the `@callee_prefix` capture if present; `node_text` it into a `String`; set `Some(...)`. Absent → `None`.
- [ ] 4.2 Update the cached `SingleCaptureQuery` struct (or replace with a dual-capture struct) to carry both the `@callee` capture index and the optional `@callee_prefix` capture index.
- [ ] 4.3 Validation tests in `src/structure/parse/validation_tests.rs`: each language's call query must compile and expose `@callee` (existing) plus optional `@callee_prefix` on method/qualified patterns.

## 5. Stage 4: scope map and scoring

- [ ] 5.1 In `src/pipeline/structural/stage4.rs`, add a local `HashMap<FileNodeId, HashSet<FileNodeId>>` populated as `Imports` edges are emitted (inside the existing per-file loop, before the `Calls` sub-loop starts).
- [ ] 5.2 Add a helper `fn score_candidate(call_ref: &ExtractedCallRef, candidate: &SymbolMeta, importing_file_id: FileNodeId, imports: &HashSet<FileNodeId>) -> i32` that applies the scoring rubric from `design.md` D2.
- [ ] 5.3 Refactor the call-resolution loop at lines 188–216:
  - Replace the bare `for &callee_id in candidates` with a scoring pass that collects `(callee_id, score)` tuples.
  - Apply the cutoff rules: `top_score > 0`, unique top OR tied at ≥ 80.
  - Emit edges only for candidates that pass.
- [ ] 5.4 Add `SymbolMeta` (or equivalent) to the name index so the scoring function has `{ file_id, visibility, kind, qualified_name }` without a per-candidate graph lookup.
- [ ] 5.5 Populate `SymbolMeta` from `graph.all_symbol_names()` — extend that query to return the extra fields, or add a sibling `all_symbol_scope_meta()`.

## 6. Logging and telemetry

- [ ] 6.1 Inside the call-resolution scoring helper, emit `tracing::debug!` on tie-emit (candidates, scores) and dropped-weak (candidates, scores) cases.
- [ ] 6.2 Accumulate per-file counters: `calls_resolved_uniquely`, `calls_resolved_ambiguous`, `calls_dropped_weak`, `calls_dropped_no_candidates`.
- [ ] 6.3 Emit a `tracing::trace!` rollup at end of `run_cross_file_resolution` with totals across all files.

## 7. Tests

- [ ] 7.1 Extend `src/pipeline/structural/tests/edges.rs` with the Rust suite from design.md D5: `rust_calls_resolve_to_imported_module`, `rust_private_fn_not_called_from_sibling`, `rust_pub_crate_fn_callable_within_crate`, `rust_ambiguous_short_name_dropped`.
- [ ] 7.2 Add Python suite: `python_method_call_on_imported_class`, `python_underscore_private_not_callable_from_outside`.
- [ ] 7.3 Add TypeScript suite: `ts_export_fn_callable_via_import`, `ts_non_exported_fn_not_callable`.
- [ ] 7.4 Add Go suite: `go_capitalized_fn_callable_across_packages`, `go_lowercase_fn_not_callable_cross_package`.
- [ ] 7.5 Run existing edge tests; confirm no recall regression on the fixtures that rely on unique-short-name.

## 8. Docs

- [ ] 8.1 Update the stage 4 module doc (`src/pipeline/structural/stage4.rs:9–26`) "Approximate resolution contract" section to describe the score/scope rules instead of "match by last `::` component, fan out to all candidates".
- [ ] 8.2 Update `AGENTS.md` "Structural pipeline stage status" bullet 4 to reflect scoped resolution.
- [ ] 8.3 Update `AGENTS.md` "Adding a new language" section to include `call_mode_map` and the `@callee_prefix` capture as required touch-points.

## 9. Verification

- [ ] 9.1 `make check` passes (fmt, clippy, parallel tests).
- [ ] 9.2 `cargo test --test mutation_soak -- --ignored --test-threads=1` — confirm the writer-path changes from the expanded `all_symbol_names` query (if added) do not regress the soak suite.
- [ ] 9.3 Re-run baseline metrics from task 1 on synrepo, flask, and axios. Document the delta in `notes.md`:
  - Total `Calls` edges dropped.
  - Share of remaining edges that are uniquely resolved (should be higher).
  - Spot-check a previously-noisy short name (e.g. `map`) and confirm the surviving edges are plausible.
- [ ] 9.4 Card smoke test: run `synrepo export --deep` on a non-trivial file; inspect a `NeighborhoodCard` that previously had dozens of noisy `Calls` entries and confirm the entries are now defensible.

## 10. Archive

- [ ] 10.1 Run `openspec validate stage4-call-scope-narrowing-v1 --strict`.
- [ ] 10.2 Invoke `opsx:archive` with change id `stage4-call-scope-narrowing-v1`.
