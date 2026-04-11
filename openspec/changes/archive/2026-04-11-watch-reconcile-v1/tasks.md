## 1. Implement single-writer watch and reconcile foundations

- [x] 1.1 Implement the first watch-triggered update loop with bounded event coalescing over the existing structural compile path
- [x] 1.2 Implement the initial single-writer safety model for standalone and future daemon-assisted runtime writes
- [x] 1.3 Add focused tests for lock conflicts, coalesced update behavior, and reconcile fallback after missed watcher coverage

## 2. Expose operational status and maintenance behavior

- [x] 2.1 Implement a small operational diagnostics surface for reconcile health, stale runtime state, and writer ownership
- [x] 2.2 Implement the first narrow maintenance hooks that consume the storage-compatibility contract for cleanup or compaction behavior
- [x] 2.3 Add tests for diagnostics output and maintenance behavior under representative unhealthy runtime conditions

## 3. Tighten contracts and validation

- [x] 3.1 Align watch, reconcile, and runtime-ops comments with the single-writer and reconcile-backstop contract
- [x] 3.2 Confirm the change remains sequenced after `structural-pipeline-v1` and does not absorb producer, Git-intelligence, or overlay-refresh work
- [x] 3.3 Validate the change with `openspec validate watch-reconcile-v1 --strict --type change`
