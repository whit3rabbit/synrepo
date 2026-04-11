## Why

Once `structural-pipeline-v1` begins populating the graph automatically, synrepo will need a trustworthy way to stay current under normal repository churn. Watch and reconcile behavior is the next Milestone 2 slice because a graph that only updates during explicit init or rebuild flows will drift quickly in real use.

## What Changes

- Define the first watcher and reconcile contract for keeping the substrate and graph current under local repository changes.
- Define single-writer operational behavior for standalone CLI and future daemon-assisted operation.
- Define the first operational status and failure-recovery surface for stale or unhealthy runtime state.
- Define a narrow initial maintenance surface for coalescing events, reconcile fallback, and basic runtime cleanup or compaction hooks.

## Capabilities

### New Capabilities
- None.

### Modified Capabilities
- `watch-and-ops`: sharpen watcher, reconcile, single-writer, diagnostics, and maintenance behavior into an implementation-ready Milestone 2 contract

## Impact

- Affects future watch and reconcile runtime code, daemon or standalone locking behavior, and operational diagnostics surfaces
- Builds directly on `structural-pipeline-v1` and the existing storage-compatibility contract
- Adds or updates tests for event coalescing, reconcile fallback, locking, and operational health behavior
- Does not change graph epistemics, cards, overlay behavior, or Git-history mining directly
