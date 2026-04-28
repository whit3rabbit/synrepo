# Symbol-to-symbol Calls edges — tasks

## 1. Research and scoping

- [x] Audit consumers of existing `EdgeKind::Calls` edges (drift scoring, public-API cards, explain queries, card compilers). File `src/structure/graph/edge.rs` is the starting point; grep for `EdgeKind::Calls` across the workspace.
- [x] Decide widen-vs-replace. Preferred: widen (keep file-scoped edges in the transition window, add symbol-scoped edges alongside). Document the decision in `design.md`.
- [x] Write the scoring rubric for ambiguous overloads (Python `self.method` with inheritance, TS class-method dispatch). Add it to `design.md` Decision 2.

## 2. Data model changes

- [x] Extend `ExtractedCallRef` in `src/structure/parse/extract/mod.rs` with the caller symbol's `(qualified_name, body_hash)` identifier so stage 4 can resolve to `NodeId::Symbol(caller)` instead of `NodeId::File(importer)`.
- [x] Thread the caller through the extractor's call-ref collection loop, propagating the symbol that encloses each call-expression node.

## 3. Resolver changes

- [x] In `src/pipeline/structural/stage4/`, add body-scope resolution:
  - [x] Lookup in in-file symbol scope first.
  - [x] Fall back to imported-file symbol scope (reuse `imports_map`).
  - [x] Score ambiguous matches per Decision 2.
- [x] Emit `Edge { from: NodeId::Symbol(caller), to: NodeId::Symbol(callee), kind: EdgeKind::Calls, ... }` on resolution.

## 4. Graph surface

- [x] Populate `SymbolCard.callers` and `SymbolCard.callees` by walking the new `Calls` edges in `src/surface/card/compiler/symbol.rs`.
- [x] Update `EdgeKind::Calls` docstring in `src/structure/graph/edge.rs` to document the widened semantics (file-scoped AND symbol-scoped endpoints are valid).

## 5. Tests

- [x] Per-language stage-4 tests asserting symbol-to-symbol `Calls` edges land:
  - [x] Rust (`src/pipeline/structural/tests/edges/symbol_calls.rs`)
  - [x] Python (`src/pipeline/structural/tests/edges/symbol_calls.rs`)
  - [x] TypeScript (`src/pipeline/structural/tests/edges/symbol_calls.rs`)
  - [x] Go (`src/pipeline/structural/tests/edges/symbol_calls.rs`)
- [x] Card-level tests verifying `SymbolCard.callers`/`callees` populate.
- [x] Transition test: file-scoped `Calls` remain valid during dual-emission, and symbol-scoped edges populate alongside.

## 6. Migration and compatibility

- [x] Verify the retired-edge flow picks up old parser-owned Calls edges and advances `retired_at_rev` as expected. Confirm `compact_retired_observations` cleans them up after `retain_retired_revisions` cycles.
- [x] Identity model sanity check: when a caller symbol's `body_hash` changes, its owning `(caller, callee)` edges must invalidate via the retirement flow. Add a focused test.

## 7. Documentation

- [x] Update `docs/FOUNDATION.md:121` to flip the `callers/callees` status from "approximate" to "shipped".
- [x] Add a section to `docs/ARCHITECTURE.md` describing body-scope resolution if it warrants one (the file covers stage-4 today).
