## 1. Fixture Contract

- [x] 1.1 Define the checked-in benchmark task JSON schema and document required fields.
- [x] 1.2 Add fixture validation for missing query text, empty expected targets, and unknown target kinds.
- [x] 1.3 Add representative tasks under `benches/tasks/` for routing, symbol lookup, impact, and test-surface discovery.

## 2. Benchmark Report

- [x] 2.1 Extend `bench context --json` with stable fields for baseline kind, target hits, target misses, stale rate, and task category.
- [x] 2.2 Keep existing raw file tokens, card tokens, reduction ratio, latency, and returned targets compatible.
- [x] 2.3 Add focused tests for hit, miss, stale, and empty-fixture behavior.

## 3. Claim Gating

- [x] 3.1 Update README or release guidance so numeric context-savings claims must cite benchmark dimensions.
- [x] 3.2 Add a small golden-output fixture or snapshot for the stable JSON report shape.
- [x] 3.3 Ensure benchmark output does not write graph facts or overlay records.

## 4. Verification

- [x] 4.1 Run `synrepo bench context --tasks "benches/tasks/*.json" --json`.
- [x] 4.2 Run focused cargo tests for benchmark parsing and report generation.
- [x] 4.3 Run `openspec validate context-benchmark-evidence-pack-v1`.
- [x] 4.4 Run `openspec status --change context-benchmark-evidence-pack-v1 --json` and confirm `isComplete: true`.
