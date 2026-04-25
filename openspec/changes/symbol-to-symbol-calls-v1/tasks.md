# Symbol-to-symbol Calls edges — tasks

## 1. Research and scoping

- [ ] Audit consumers of existing `EdgeKind::Calls` edges (drift scoring, public-API cards, explain queries, card compilers). File `src/structure/graph/edge.rs` is the starting point; grep for `EdgeKind::Calls` across the workspace.
- [ ] Decide widen-vs-replace. Preferred: widen (keep file-scoped edges in the transition window, add symbol-scoped edges alongside). Document the decision in `design.md`.
- [ ] Write the scoring rubric for ambiguous overloads (Python `self.method` with inheritance, TS class-method dispatch). Add it to `design.md` Decision 2.

## 2. Data model changes

- [ ] Extend `ExtractedCallRef` in `src/structure/parse/extract/mod.rs` with the caller symbol's `(qualified_name, body_hash)` identifier so stage 4 can resolve to `NodeId::Symbol(caller)` instead of `NodeId::File(importer)`.
- [ ] Thread the caller through the extractor's call-ref collection loop, propagating the symbol that encloses each call-expression node.

## 3. Resolver changes

- [ ] In `src/pipeline/structural/stage4/`, add body-scope resolution:
  - [ ] Lookup in in-file symbol scope first.
  - [ ] Fall back to imported-file symbol scope (reuse `imports_map`).
  - [ ] Score ambiguous matches per Decision 2.
- [ ] Emit `Edge { from: NodeId::Symbol(caller), to: NodeId::Symbol(callee), kind: EdgeKind::Calls, ... }` on resolution.

## 4. Graph surface

- [ ] Populate `SymbolCard.callers` and `SymbolCard.callees` by walking the new `Calls` edges in `src/surface/card/compiler/symbol.rs`.
- [ ] Update `EdgeKind::Calls` docstring in `src/structure/graph/edge.rs` to document the widened semantics (file-scoped AND symbol-scoped endpoints are valid).

## 5. Tests

- [ ] Per-language stage-4 tests asserting symbol-to-symbol `Calls` edges land:
  - [ ] `src/pipeline/structural/tests/edges/rust.rs`
  - [ ] `src/pipeline/structural/tests/edges/python.rs`
  - [ ] `src/pipeline/structural/tests/edges/typescript.rs`
  - [ ] `src/pipeline/structural/tests/edges/go.rs`
- [ ] Card-level tests verifying `SymbolCard.callers`/`callees` populate.
- [ ] Migration test: existing file-scoped `Calls` edges retire on first compile after the new code lands; new symbol-scoped edges populate alongside.

## 6. Migration and compatibility

- [ ] Verify the retired-edge flow picks up old file-scoped Calls edges and advances `retired_at_rev` as expected. Confirm `compact_retired_observations` cleans them up after `retain_retired_revisions` cycles.
- [ ] Identity model sanity check: when a caller symbol's `body_hash` changes, its owning `(caller, callee)` edges must invalidate via the retirement flow. Add a focused test.

## 7. Documentation

- [ ] Update `docs/FOUNDATION.md:121` to flip the `callers/callees` status from "approximate" to "shipped".
- [ ] Add a section to `docs/ARCHITECTURE.md` describing body-scope resolution if it warrants one (the file covers stage-4 today).
