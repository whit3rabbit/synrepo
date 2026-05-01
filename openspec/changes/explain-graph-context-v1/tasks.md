## 1. Shared Context Builder

- [x] 1.1 Add `src/pipeline/explain/context/` with `CommentaryContextOptions` and shared context assembly for file and symbol targets.
- [x] 1.2 Include degree-one graph blocks for imports, imported-by files, callers, callees, visible/exported symbols, governing decisions, tests, co-change partners, markers, and related source snippets.
- [x] 1.3 Enforce `max_input_tokens` by trimming optional blocks before provider calls.

## 2. Integration

- [x] 2.1 Route repair/sync commentary context through the shared builder with a thin compatibility wrapper.
- [x] 2.2 Route `GraphCardCompiler::refresh_commentary` through the shared builder and configured commentary budget.
- [x] 2.3 Preserve graph-only input boundaries and avoid overlay commentary, proposed links, or explain docs.

## 3. Verification

- [x] 3.1 Add unit tests for context builder neighborhood content, export summaries, trimming, ordering, and trust-boundary behavior.
- [x] 3.2 Update commentary refresh tests to assert the prompt contains graph neighborhood context.
- [x] 3.3 Run targeted commentary tests, OpenSpec status, and `make ci-lint`.
