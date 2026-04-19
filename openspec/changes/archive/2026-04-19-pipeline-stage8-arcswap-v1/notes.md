## Approval Notes

- 2026-04-18: Operator confirmed the snapshot scope is the full in-memory graph, not a file-plus-symbol-only index.
- 2026-04-18: Operator approved lowering the default snapshot memory ceiling from the proposal's 500 MB to 128 MB.
- 2026-04-18: Verified on branch `main` that prerequisite `symbol-body-hash-column-v1` is already shipped.
  Evidence: commit `a18755a3f4df69c586f99610facf8f9d9558eae3` archives the change and the live codebase contains the dedicated `body_hash` column migration and downstream usage.

## Benchmark Notes

- 2026-04-18: Benchmarked `synrepo_module_card` against the `src/surface` directory on this repo with 10 iterations per mode and median wall-clock time.
- Baseline method: forced the SQLite read path with `SYNREPO_DISABLE_GRAPH_SNAPSHOT=1`.
- Snapshot method: built and published an in-process `ArcSwap<Graph>` snapshot, then measured the same handler without the override.
- Result: SQLite median `1.802 ms`, snapshot median `0.175 ms`, measured speedup `10.283x`.
- Snapshot build timing: median `221.410 ms` across 5 `Graph::from_store` materializations against the live synrepo graph.
- Snapshot size/counts at measurement time: `25,364,353` bytes, `308` files, `2901` symbols, `13,845` active edges.

## Spec Notes

- 2026-04-18: Inspected `openspec/specs/foundation/spec.md` for a Stage 8 row or TODO marker. No Stage 8 row is present there, so task 9.5 is satisfied by inspection rather than a file edit.
