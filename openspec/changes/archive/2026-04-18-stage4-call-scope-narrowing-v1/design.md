## Context

Stage 4 call resolution today is short-name lookup with fan-out. Every candidate with a matching short name becomes an edge. This is the simplest thing that could work, and was deliberately chosen for the phase-1 cut. It has outlived its usefulness: short generic names (`map`, `get`, `new`, `parse`, `build`, `send`) produce edges to every symbol in the repo with that name. Downstream consumers — card `Calls`-edge summaries, neighborhood expansion, the MCP `synrepo_graph_query` tool — treat these edges as authoritative and show users hundreds of spurious connections.

The parser already sees enough to do better. For `foo.map(|x| ...)`, tree-sitter captures `@callee = map` and we can also capture `@callee_prefix = foo`. For `Iterator::map`, we capture `@callee_prefix = Iterator`. For `HashMap::new()`, `@callee_prefix = HashMap`. These are the scope hints. Combined with:

- The file's own `Imports` edges (already emitted earlier in stage 4),
- Each candidate's `Visibility` (from `cross-language-symbol-visibility-v1`),

we can score candidates and emit edges only for high-confidence matches.

This is not full type inference and never will be in this project. It is scope-narrowing — the same principle that makes the existing `Imports` resolvers tolerable: use the language's declaration surface to reject candidates that cannot possibly be the target.

## Goals / Non-Goals

**Goals:**

- Reduce `Calls` edge fan-out on generic short names (`map`, `get`, `new`, `send`, `build`) to the point where neighborhood expansion is useful.
- Preserve recall for unambiguous calls — if there is exactly one candidate, the edge still lands.
- Keep the phase-1 "approximate" contract: unresolved calls remain silently skipped, not errored.

**Non-Goals:**

- No type inference. The receiver's type is treated as an opaque name. `foo.map` gives us the string `"foo"`, not `Vec<String>`.
- No trait-impl resolution. `T::method()` where `T` is a generic parameter stays unresolved.
- No multi-hop call graph. We resolve direct calls; call chains are the downstream consumer's job.
- No restructuring of the `ExtractedCallRef → Edge` data path beyond adding fields.

## Decisions

### D1: Capture `@callee_prefix` and `is_method` in the parser, score in stage 4

The parser has the AST; it is the cheapest place to get the prefix text. Passing scoring logic into the parser would entangle structural and pipeline concerns — the parser would need the importing file's imports, the project's visibility map, etc. Keep the parser scope-free; stage 4 gets the data and the scoring context together.

`ExtractedCallRef`:

```rust
pub struct ExtractedCallRef {
    pub callee_name: String,
    /// The receiver or qualifier text at the call site.
    ///
    /// For `foo.bar()` this is `"foo"`. For `Type::method()` this is `"Type"`.
    /// For `bar()` (free function) this is `None`.
    pub callee_prefix: Option<String>,
    /// True when the call site is a method/attribute call (`obj.method()`),
    /// false when it's a free function call (`fn_name()`).
    pub is_method: bool,
}
```

Per-language extraction rules (added to the `call_query` definitions in `src/structure/parse/language.rs`):

- **Rust**:
  - `(call_expression function: (identifier) @callee)` → `is_method = false`, `callee_prefix = None`.
  - `(call_expression function: (field_expression value: (_) @callee_prefix field: (field_identifier) @callee))` → `is_method = true`, `callee_prefix = text of value`.
  - `(call_expression function: (scoped_identifier path: (_) @callee_prefix name: (identifier) @callee))` → `is_method = false`, `callee_prefix = text of path`.
- **Python**:
  - `(call function: (identifier) @callee)` → `is_method = false, callee_prefix = None`.
  - `(call function: (attribute object: (_) @callee_prefix attribute: (identifier) @callee))` → `is_method = true, callee_prefix = text of object`.
- **TypeScript/TSX**:
  - `(call_expression function: (identifier) @callee)` → `is_method = false, callee_prefix = None`.
  - `(call_expression function: (member_expression object: (_) @callee_prefix property: (property_identifier) @callee))` → `is_method = true, callee_prefix = text of object`.
- **Go**:
  - `(call_expression function: (identifier) @callee)` → `is_method = false, callee_prefix = None`.
  - `(call_expression function: (selector_expression operand: (_) @callee_prefix field: (field_identifier) @callee))` → `is_method = true, callee_prefix = text of operand`.

Each language's query set now has a definition-query-style association between a pattern index and metadata. We already have a `kind_map` for definitions; call queries get a `call_mode_map: &[CallMode]` where `CallMode ∈ { Free, Method }`. The `extract_call_refs` loop reads the mode by `m.pattern_index` and populates `is_method`.

### D2: Score candidates with an additive rubric; cutoff at top-of-two rules

The scoring table from the proposal is repeated here with rationale:

| Signal | Score | Why |
|--------|-------|-----|
| Same file | +100 | Always callable. Ground truth. |
| Imported file (direct or module-level) | +50 | Strongest positive signal; mirrors how `Imports` edges narrow the candidate set. |
| `visibility == Public` | +20 | Callable project-wide; still weaker than file-scoped evidence. |
| `visibility == Crate` (Rust same-crate only) | +10 | Callable within the crate; requires same crate root match. |
| `visibility == Private` cross-file | -100 | Hard reject. Private means unreachable. |
| `is_method` AND candidate `kind == Method` | +30 | Kind match strengthens an otherwise ambiguous name. |
| `!is_method` AND candidate `kind ∈ { Function, Constant }` | +30 | Free-call kind match. |
| `callee_prefix` equals a segment of candidate's qualified name | +40 | String-match scoping; the strongest signal we have without type inference. |

**Cutoff rules (apply after scoring all candidates):**

- If the top score is ≤ 0, emit no edge.
- If exactly one candidate has the top score, emit an edge to that candidate.
- If multiple candidates tie at the top and the top score ≥ 80, emit edges to all tied candidates (this is the "strongly-scoped, still ambiguous" case; two equally-good matches should both be recorded).
- If multiple candidates tie at the top with score < 80, emit no edge (this is the "weak ambiguity" case; the original fan-out was wrong here).

80 is chosen as "same file OR (imported + any kind/prefix bonus)". It admits the common cases while rejecting short-name collisions that have nothing to go on but visibility.

Rationale for additive scoring over a fixed tree of rules:

- Easy to tune individual signals based on metrics (if prefix match turns out to be noisier than expected, drop its weight).
- Explains itself in logs — log the (candidate, score-breakdown) on low-confidence matches.
- Avoids the combinatorial explosion of "rule 1 AND rule 2 OR (rule 3 AND NOT rule 4)" logic we would otherwise accumulate.

### D3: Build importing-file scope via in-memory `Imports` edges

Stage 4 already emitted `Imports` edges for this file earlier in the same call to `run_cross_file_resolution` (imports are resolved before calls in the loop at line 188). Instead of reading them from the graph (which is inside the same transaction and expensive per-call), maintain an in-memory map:

```rust
let mut imports_by_file: HashMap<FileNodeId, HashSet<FileNodeId>> = HashMap::new();
```

Populated as each `Imports` edge is emitted inside the same loop, before the `Calls` loop runs for that file. This makes the scope lookup O(1) per call_ref.

### D4: Logging and metrics

Add `tracing::debug!` at three points:

- When a call resolves uniquely with top score ≥ 80: no-op (too verbose; debug is fine).
- When a call resolves via the tie-emit-all rule: log `(call_site, top_score, n_candidates)`.
- When a call is dropped with candidates present but top score below cutoff: log `(call_site, all_candidates, all_scores)` once per file, batched.

After the call loop for a file, accumulate:

- `calls_resolved_uniquely: usize`
- `calls_resolved_ambiguous: usize` (tie at ≥ 80)
- `calls_dropped_weak: usize`
- `calls_dropped_no_candidates: usize` (matches `name_index.get()` returned empty)

Emit a per-file `tracing::trace!` with the four counters. Stage 4 already emits summary telemetry at the end of the function; add a global rollup.

This replaces the current silent drop with actionable signal during debugging.

### D5: Tests

Per-language positive and negative coverage in `src/pipeline/structural/tests/edges.rs`:

- **Rust**
  - `rust_calls_resolve_to_imported_module` — `file_a.rs` imports `file_b::transform`; a call to `transform()` in `file_a` emits exactly one `Calls` edge to `file_b::transform`, not to an unrelated `transform` in `file_c`.
  - `rust_private_fn_not_called_from_sibling` — `fn private_helper` in `a.rs` is called from `b.rs`; no edge is emitted.
  - `rust_pub_crate_fn_callable_within_crate` — `pub(crate) fn foo` in `lib.rs` is called from `helpers.rs`; edge emitted.
  - `rust_ambiguous_short_name_dropped` — two unrelated modules define `map`; a call site in a third file with no imports emits zero edges.
- **Python**
  - `python_method_call_on_imported_class` — `from util import User; User.greet()` emits an edge to `util.User.greet`, not to a random other `greet()`.
  - `python_underscore_private_not_callable_from_outside` — `_helper` in `a.py` called from `b.py`; no edge emitted.
- **TypeScript**
  - `ts_export_fn_callable_via_import` — `export function handle()` in `a.ts` called from `b.ts` after `import { handle } from './a'`; edge emitted.
  - `ts_non_exported_fn_not_callable` — same setup but without `export`; no edge emitted.
- **Go**
  - `go_capitalized_fn_callable_across_packages` — `pkg/util.Handle` called from `main.go`; edge emitted.
  - `go_lowercase_fn_not_callable_cross_package` — `pkg/util.helper` called from `main.go`; no edge emitted.

Baseline regression: keep the existing tests in `src/pipeline/structural/tests/edges.rs`; confirm that previously-passing calls that match via unique-short-name still resolve (no recall regression on uncomplicated repos).

## Risks / Trade-offs

- **Short-name calls with no prefix and no imports will drop more edges than today**. This is the intended behavior — those edges were wrong — but it will show up as a drop in raw `Calls` count, which a benchmark could misread as a regression. Mitigation: record the before/after breakdown (unique vs dropped) in the task log so the metric is interpretable.

- **Prefix match is string-based, not type-based**. `HashMap::new()` scored against a candidate named `Map::new` will miss. Conversely, `HashMap::new()` scored against a deliberate `HashMap` in a different module (e.g., a user's own `HashMap` type) will incorrectly score-match. The second case is rare; the first is what type inference would fix and is explicitly out of scope. The -100 private-visibility gate handles most cross-module collisions.

- **`Imports` edges are precondition for cross-file resolution**. Stage 4 emits `Calls` after `Imports` per-file, so the imports-emit-and-use loop is single-pass. If a future refactor splits stages into separate transactions, this change's assumptions break. Mitigation: the in-memory `imports_by_file` map decouples the resolver from SQLite round-trips, so a stage split would only need to serialize-and-load that map.

- **Scoring tuning debt**: numeric weights are picked by intuition. Mitigation: the task log should record before/after metrics on synrepo itself and at least one external corpus (e.g., a medium-size Python or TS repo). If a weight is visibly wrong post-ship, a follow-up change tunes it. Do not block this change on empirical tuning — ship the shape, iterate weights.

- **Python dunder methods**: `__init__`, `__enter__`, etc. are called by protocol, not by name at a site the parser sees. The proposal does not try to model these. Call-resolution recall for dunder methods is a separate concern.

## Migration Plan

Single PR, but sequence-sensitive: this change **must land after `cross-language-symbol-visibility-v1`**. Without `Visibility` on symbols, the scoring rubric's visibility rows are unimplementable.

No data migration. Existing `Calls` edges are retired on the next reconcile and re-emitted under the new rules. The edge-count drop is visible in `synrepo status --full` as a delta in `graph_edge_count`; users who treat this as a health signal need to know it is expected.

Rollback: revert the scoring block and the new `ExtractedCallRef` fields; old short-name fan-out is restored. No storage changes to unwind.

## Open Questions

- **O1**: Should the tie-emit-all threshold be 80, 90, or configurable? Start with 80. Revisit after running on real corpora.
- **O2**: Should dropped-weak calls be surfaced to users somehow (e.g., in `synrepo status --full` as "structural calls skipped: 42")? Current proposal: telemetry only, not a user-visible count, because the number is meaningless without context. Flag for review.
- **O3**: Go method resolution via `(selector_expression)` — does tree-sitter-go's grammar actually expose the operand as a typed child we can match on? Verify during implementation; if the shape differs, adjust the query. The scoring layer is unaffected.
