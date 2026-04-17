## Context

The structural parser has three load-bearing responsibilities in synrepo:

1. Extract symbols and within-file structure from supported source files.
2. Produce stage-4 inputs (`ParseOutput.call_refs`, `ParseOutput.import_refs`) for cross-file resolution.
3. Tolerate imperfect source while remaining deterministic across runs.

Supported languages today: Rust, Python, TypeScript, TSX, Go. Queries for symbol definitions, call sites, and imports are embedded directly in Rust as string literals returned by `definition_query()`, `call_query()`, and `import_query()` on `Language` in `src/structure/parse/language.rs`. The query cache in `src/structure/parse/extract/mod.rs` soft-fails: a query that does not compile stays `None` in the cache and `parse_file` returns partial or empty output without surfacing the compile error. `kind_for_pattern()` maps query pattern indexes to `SymbolKind` and silently falls back to `SymbolKind::Function` when the index is out of range. Stage 4 is intentionally approximate — unresolved names are skipped without error — which means parser regressions can masquerade as ordinary unresolved references.

The failure mode we want to eliminate is silent parser degradation after grammar crate upgrades, tree-sitter query edits, supported-language expansion, or pattern ordering changes. Runtime must remain resilient on user machines, but CI must be strict enough to catch these regressions before they ship.

## Goals / Non-Goals

**Goals:**

- Make tree-sitter query compilation a hard CI contract for every supported language.
- Pin the query pattern-index → `SymbolKind` mapping per language so ordering drift fails loudly.
- Give every supported language — including TSX, which currently has no dedicated fixtures — explicit parser extraction coverage.
- Treat `call_refs` and `import_refs` as first-class, tested `ParseOutput` fields, not incidental data.
- Pin the intended behavior of `parse_file` on malformed or unsupported inputs.
- Lock in the current stage-4 approximate-resolution contract (ambiguous-name fanout, unresolved-skip, TS relative and Python dotted imports, Rust `use` skipping) via integration tests.

**Non-Goals:**

- Changing the stage-4 resolution contract itself. Phase-1 approximation stays; this change only locks it down with tests.
- Replacing tree-sitter or switching grammar crates.
- Improving semantic cross-file resolution quality.
- Broad structural pipeline redesign.
- Promoting embedded queries to external `.scm` assets. That is evaluated as optional cleanup and deferred unless the validation surface demands it.
- Changing runtime error handling in ways that would escalate ordinary malformed user source to fatal errors.

## Decisions

### Decision 1: Keep runtime permissive; make tests strict.

Runtime parse behavior stays resilient: missing query caches, malformed source, and unsupported extensions continue to produce `None` / partial outputs rather than errors. Strictness is scoped to the test suite and therefore to CI.

Rationale: users routinely run synrepo against repos with broken files, generated code, or partial rewrites. Escalating those to fatal errors would regress product UX. But the query-compilation and pattern-index invariants are developer-facing — they only break when we change code or dependencies — so failing loud in tests is the right scope.

Alternatives considered:

- Fail at runtime on query-compile error. Rejected: punishes end users for a developer-side regression.
- Add a `--strict` flag. Rejected: adds surface area without addressing the root need, which is CI enforcement.

### Decision 2: Treat `call_refs` and `import_refs` as first-class tested outputs.

`ParseOutput` already exposes both fields, and stage 4 consumes them directly. They get per-language fixture tests that assert the raw references extracted, independent of stage-4 resolution.

Rationale: stage 4 skips unresolved names, which masks parser regressions. Testing the parser's stage-4-facing outputs directly severs that camouflage.

### Decision 3: Validate every supported language explicitly via a language enumeration.

Query validation, pattern-index mapping validation, and fixture coverage all iterate the finite supported-language set. Adding a new `Language` variant must force updates to the validation surfaces or the tests will not compile.

Rationale: catches "added a language, forgot to add coverage" at compile time.

Alternatives considered:

- One test per language hand-written. Rejected: invites drift when a new language is added.

### Decision 4: Pin pattern-index → `SymbolKind` mapping in tests rather than change runtime fallback behavior.

The runtime may keep a conservative fallback (`SymbolKind::Function`) for forward-compatibility with grammar versions we have not tested. Tests assert the exact mapping per language so drift cannot pass CI. Whether to harden the runtime path (e.g. `debug_assert!`) is a follow-up design call inside task 2, not a prerequisite.

Rationale: the regression risk is drift, not fallback. A test that pins the mapping catches drift before release without changing runtime behavior for unknown patterns.

### Decision 5: Pin malformed-source behavior by contract, not by snapshot.

The intended semantics:

- Unsupported extension → `parse_file` returns `None`.
- Supported extension, syntactically malformed → `parse_file` returns `Some(ParseOutput)` with best-effort extraction. Must not panic. Must be deterministic given identical input.
- Internal parser/query invariant break (e.g. query compile failure reached at runtime) → runtime stays soft; CI catches the invariant break in the query-validation test suite.

Tests assert contract-level properties (non-panic, `Some`/`None`, determinism, bounded output) rather than exact-match snapshots, to avoid churn when grammars evolve.

Rationale: we want to lock behavior, not grammar implementation details.

### Decision 6: Fixtures live in-tree as Rust-embedded strings or a dedicated fixtures module, not as `.scm` assets.

Query strings remain in `src/structure/parse/language.rs` for this change. If validation tests start duplicating query metadata, we may centralize metadata in a single table — but moving queries to external `.scm` files is explicitly deferred as optional cleanup (task 8 in `tasks.md`).

Rationale: the immediate failure mode is "no CI signal," not "queries are hard to review." Solving signal first keeps the change scoped.

Alternatives considered:

- Move queries to `queries/<lang>/*.scm` and embed via `include_str!`. Good for reviewability, but adds an asset pipeline and does not by itself produce the missing CI signal. Parked for a follow-up change if the test suite motivates it.

### Decision 7: Stage-4 tests validate contracts, not implementation paths.

Stage-4 integration tests assert observable behavior: ambiguous-name → multiple candidate `Calls` edges, unresolved → no edge and no error, TS relative path resolution, Python dotted resolution, Rust `use` last-name skipping. They do not lock implementation-specific internals that could shift without changing the contract.

Rationale: the change is about preventing silent degradation, not freezing the current implementation.

## Risks / Trade-offs

- [Over-hardening runtime escalates malformed user source to fatal errors.] → Runtime paths stay permissive; strictness is test-only. All new assertions live behind `#[cfg(test)]` or in dedicated test modules.
- [Snapshot-heavy tests become brittle and create maintenance churn on grammar bumps.] → Prefer focused assertions (specific symbol name, kind, count bounds) over whole-`ParseOutput` snapshots. Use `insta` only where the output surface is intentionally stable.
- [Query assets and code drift apart if queries move to `.scm`.] → Move is deferred. If undertaken later, a single validation entrypoint must enumerate supported languages and query assets together.
- [Pinning stage-4 approximate-resolution behavior could block a future refinement.] → Tests assert the current contract. When stage 4 is intentionally upgraded, those tests are updated as part of that change — which is the correct signal that the contract moved.
- [Adding a new `Language` variant is only partially enforced.] → Match-based iteration in validation helpers forces `Language` exhaustiveness; fixture coverage is still developer discipline. Mitigation: a "supported languages covered by fixtures" test that fails if a variant has no fixture registered.
- [Test-only strictness gives a false sense of security if `cargo test` is not run in CI.] → Rely on existing `make check` (fmt + clippy + test) which CI already runs. No new CI wiring required.

## Migration Plan

No runtime migration is required. The change is additive in tests and leaves user-visible behavior unchanged.

Rollout sequencing:

1. Land query-validation suite and pattern-index mapping tests first. These are the highest-value, lowest-risk additions and will flush any currently-silent regressions before fixture work piles on.
2. Add per-language fixtures in small batches (one language per commit) so failures are attributable.
3. Add stage-4 integration tests last, since they build on parser fixtures.

Rollback: revert the change. Runtime behavior is unaffected, so rollback is test-only.

## Open Questions

- Should `kind_for_pattern()` gain a `debug_assert!` or `cfg(test)` panic in addition to test-pinned mappings? Default position: no, unless a concrete drift incident justifies it. Decided inside task 2.
- Does TSX need a dedicated `Language::Tsx` fixture file layout separate from TypeScript, or can both share a `fixtures/typescript/` tree differentiated by extension? Default position: share a tree, separate by extension; revisit if TSX-only captures appear.
- Should malformed-source tests live alongside parser tests or under `pipeline/structural/` stage-4 tests? Default position: parser tests. Stage 4 only tests resolution behavior on a valid `ParseOutput`.

## Post-implementation notes

### Task 8.1 evaluation — queries stay embedded.

After landing sections 1–7, the validation surface does not motivate moving embedded queries to external `.scm` assets. The per-language queries are small (each fits in a few dozen lines on `Language`), developer-facing, and exercised by the new validation suite. Moving them to `queries/<lang>/*.scm` would add an asset pipeline without closing a concrete gap: the CI signal we wanted is already produced by `validation_tests.rs`. Revisit if a future change bloats the queries or wants grammar-version-pinned snapshots.

### Task 8.2 — single metadata surface.

Per-language query metadata is centralized on the `Language` enum: `Language::supported()` lists the canonical set, `definition_query()` / `call_query()` / `import_query()` return the query text, and `kind_map()` returns the pattern-index → `SymbolKind` table. Adding a new `Language` variant therefore forces updates on four co-located methods plus fixture registration; `validation_tests` and `fixture_tests` will fail loudly if any one is missed.

### Task 9.4 — shipped test counts.

- `validation_tests.rs`: 8 tests (query compile per language, capture presence, kind-map pins).
- `fixture_tests.rs`: 6 tests (one per supported language + a coverage-enforcement test).
- `qualname_tests.rs`: 7 tests (Rust impl/trait/nested module, Python methods and nested, TypeScript methods and class expression).
- `refs_tests.rs`: 15 tests (call_refs per language, import_refs per language, phase-1 negative cases).
- `malformed_tests.rs`: 8 tests (unsupported extension, malformed per language, determinism, empty input).
- `pipeline/structural/tests/edges.rs`: +5 stage-4 contract tests (ambiguous fanout, unresolved skip, TSX relative, Python dotted, Rust `use` phase-1 boundary).

Parser-behavior clarifications surfaced while writing tests:

- Python qualname treats a class body as a method-bearing scope; a class nested inside a class body is extracted with `SymbolKind::Method`. Documented as a phase-1 rough edge rather than corrected, since nested classes are rare in practice and changing the qualname walk risks drift elsewhere.
- Go's `interpreted_string_literal` capture retains surrounding double quotes in `import_refs`; stage 4 is responsible for stripping them. The refs test uses `contains("fmt")` rather than equality to pin this.
- Rust braced `use` groups (`use std::collections::{HashMap, HashSet}`), Python `from ... import Name` symbols, TS/TSX `export { X } from './y'` re-exports, and Go dot-import aliases are all intentionally not captured in phase 1. Negative tests in `refs_tests.rs` pin this so a future query expansion has to delete the negative test deliberately.
- Stage 4 does not resolve Rust `use` paths to files in phase 1. The last-name still populates `import_refs` from the parser, but no `Imports` edge is emitted. The stage-4 edge test documents this as a phase-1 boundary.
