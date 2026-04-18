## Why

Stage 4 resolves call sites to callee symbols by short-name lookup with no scope narrowing. `src/pipeline/structural/stage4.rs:119–216`:

```rust
let all_symbols = graph.all_symbol_names()?;
let mut name_index: HashMap<String, Vec<SymbolNodeId>> = HashMap::new();
for (sym_id, _file_id, qname) in &all_symbols {
    let short = qname.rsplit("::").next().unwrap_or(qname.as_str());  // line 122
    ...
}
...
for call_ref in &item.call_refs {
    let candidates = name_index
        .get(&call_ref.callee_name)
        .map(Vec::as_slice)
        .unwrap_or(&[]);

    for &callee_id in candidates {
        ...
        graph.insert_edge(edge)?;  // line 213 — emits one edge per candidate
    }
}
```

Concretely: any `.map(...)` call site in the repo connects to every symbol named `map` — `Iterator::map`, `Option::map`, `Result::map`, every test helper called `map`, every user-defined `map`. `HashMap::get` collides with `HashMap::get`, `BTreeMap::get`, `OnceLock::get`, `HashSet::get`, `Config::get`, etc. The current code emits an edge to every candidate, which produces an edge-count explosion and makes `Calls`-edge neighborhood cards useless for any generic short name.

The line 11 module doc acknowledges this as "approximate resolution (phase 1)" with the stated tradeoff that unresolved calls are silently skipped. Skipping is fine; indiscriminate fan-out is not. Phase 1 is now two years old; the parser captures enough structure (callee prefix, receiver type, importing file's imports) to do materially better without a full type-inference engine.

**Verified scope**: Import resolution in the same file is already well-scoped — Rust handles `crate::`/`self::`/`super::` with longest-match selection (lines 377–458 in the same file); Go strips the `go.mod` module prefix and fans out correctly (lines 581–616). This change targets calls only.

**Cross-link** with cross-language-symbol-visibility-v1: once symbols carry a `Visibility` field, a short-name candidate that is `Private` is not callable from outside its defining file. This is the first scope narrowing this change will lean on.

## What Changes

- **Capture call-site prefix at parse time** (Rust, TS, Python, Go). The tree-sitter call queries already match on `identifier`, `field_identifier`, and `property_identifier`. Extend the queries to also capture:
  - `@callee_prefix` — the text of the receiver / qualifier (e.g., `self`, `config`, `HashMap`, `Iterator`, the callee's containing scope).
  - `@callee_is_method` — a boolean (derived from whether the capture comes from a method-call vs free-function call pattern).
  - The receiver text is not resolved to a type in this change. It is only used as a scope-narrowing hint.
- **Add `ExtractedCallRef.callee_prefix: Option<String>` and `ExtractedCallRef.is_method: bool`** in `src/structure/parse/mod.rs:95–99`.
- **Stage 4 name-index changes** in `src/pipeline/structural/stage4.rs:113–130`:
  - Keep the short-name index, but also keep a parallel `file_index_by_symbol` so we can get back to the defining file for each candidate.
  - Build a **per-importing-file scope set**: the union of `FileNodeId` for (a) the importing file itself, (b) the files it imports (via the `Imports` edges from `stage4-rust-go-resolvers-v1`, already emitted earlier in the same compile pass, so readable via `graph.outbound_edges(importing_file_id, Some(EdgeKind::Imports))`), (c) the project's public/crate visibility set (callable from anywhere in the same compilation unit).
- **Call-resolution scoring** in `src/pipeline/structural/stage4.rs:188–216`:
  - For each `(call_ref, candidate)` pair, compute a score:
    - +100 if `candidate.file_id == importing_file_id` (same file — always callable).
    - +50 if `candidate.file_id ∈ importing_file_imported_files` (imported directly or transitively via same-module).
    - +20 if `candidate.visibility == Visibility::Public` (the candidate is legitimately callable across the project).
    - +10 if `candidate.visibility == Visibility::Crate` (Rust: callable inside the same crate; match requires the importing file to be in the same crate root as the candidate's file).
    - -100 if `candidate.visibility == Visibility::Private` and `candidate.file_id != importing_file_id` (hard-reject cross-file private calls).
    - +30 if `is_method == true` and `candidate.kind ∈ { Method }` (method call matches method-kind symbol).
    - +30 if `is_method == false` and `candidate.kind ∈ { Function, Constant }` (free-call matches non-method).
    - +40 if `callee_prefix` exactly matches one of the candidate's enclosing qualified-name components (e.g., `Iterator::map` has `Iterator` in its scope; a `Iterator::map` call with `@callee_prefix = "Iterator"` gets the bonus).
  - **Only emit an edge if the top score > 0 and** (exactly one candidate achieves the top score, OR the top score ≥ 80). This admits unambiguous hits and strongly-scoped hits, and drops everything else. A tie at a low score produces no edge rather than a fan-out.
- **Dispatch per language** — the `@callee_prefix` extractor and the call-receiver text have different tree shapes per language. Keep the scoring layer in stage 4 language-agnostic; do the per-language prefix extraction in `src/structure/parse/extract/mod.rs::extract_call_refs`.
- **Deletion of line 122's naive short-name bucketing stays the same** (the name index is still keyed by short name for the first cut); the narrowing happens after candidate lookup, not at index build time.
- **Retain the "approximate resolution is acceptable" contract** from the module doc. This is still approximate — we are reducing false positives, not guaranteeing completeness. Unresolved or ambiguous calls are still silently skipped.
- **Tests**:
  - Extend `src/pipeline/structural/tests/edges.rs` with per-language positive tests (call to an imported function should resolve to the imported file's symbol) and negative tests (call to a private same-file function from a sibling file must not emit an edge).
  - Add a Rust-specific test with two modules defining `map` — assert that a call from a file importing only one of the two lands on that one.
  - Add a TypeScript test with `foo.map` where `foo: Array<T>`; assert no edge is emitted to the user's unrelated `map()` in a different file. (This one relies on the `is_method` scoring and the absence of imports, not on type inference.)
- **Metrics**: before-and-after `Calls` edge counts on synrepo itself. Document the delta in the change's tasks.md as an acceptance criterion (expect: fewer total `Calls` edges, larger share resolvable to exactly one callee).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `structural-pipeline`: stage 4 `Calls` edge emission gains a scope-narrowing pass. Edge counts drop; precision improves. The phase-1 "approximate resolution" contract is retained — unresolved calls are still silently skipped.
- `parse`: `ExtractedCallRef` gains `callee_prefix` and `is_method`. Call queries are extended with `@callee_prefix` captures.

## Impact

- **Code**:
  - `src/structure/parse/mod.rs` — extend `ExtractedCallRef` with two new fields.
  - `src/structure/parse/language.rs` — extend per-language call queries with `@callee_prefix` captures and (where natural) method-vs-function distinguishing patterns.
  - `src/structure/parse/extract/mod.rs::extract_call_refs` — capture the new fields.
  - `src/pipeline/structural/stage4.rs:113–216` — rewrite the call-resolution inner loop to score candidates and apply the scope filter.
  - `src/pipeline/structural/tests/edges.rs` — new tests per above.
- **APIs**: No public API break. `Calls` edge emission remains in the same stage, on the same transaction, with the same contract. Consumers that iterate `Calls` edges see fewer false positives; any caller that relied on the fan-out behavior was already treating a noisy edge set as authoritative, which was not defensible.
- **Storage**: No schema change. Edge-count reduction is handled by the normal retirement / re-emission cycle on reconcile.
- **Dependencies**: None.
- **Docs**:
  - `AGENTS.md` "Stages 4–8" — update the stage 4 bullet to reflect scoped (not naive) resolution.
  - `src/pipeline/structural/stage4.rs:9–20` module doc — update the "approximate resolution contract" section to describe the score/scope rules.
- **Systems**: This change depends on `cross-language-symbol-visibility-v1` landing first (the `Visibility::Public`/`Crate`/`Private` scoring is load-bearing). Confirmed as a hard dependency in the roadmap.
