## Why

`synrepo bench context` exists, but the repo does not yet carry benchmark fixtures or release rules that make context-savings claims reproducible. This change turns the harness into product evidence by requiring checked-in tasks, stable reports, and claim gating.

## What Changes

- Add a checked-in benchmark task fixture contract for context tasks.
- Add representative fixture coverage across search, card routing, impact, and test-surface discovery.
- Extend context benchmark output with baseline comparison, hit/miss detail, stale rate, and stable JSON fields.
- Require README or release-note context-savings percentages to cite benchmark output rather than ad hoc runs.
- Keep the benchmark observational; it does not write graph facts, overlay notes, or source-derived truth.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `context-accounting`: context benchmark reporting becomes stable enough for product claims.
- `evaluation`: numeric context-savings claims require fixture-backed benchmark evidence.

## Impact

- Benchmark fixture files under `benches/tasks/`.
- `synrepo bench context` report schema and CLI tests.
- README or release documentation rules for numeric savings claims.
- No graph, overlay, MCP, or storage migration changes.
