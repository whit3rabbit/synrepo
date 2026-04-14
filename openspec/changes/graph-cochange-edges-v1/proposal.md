## Why

`EdgeKind::CoChangesWith` is defined and serialized, `GitHistoryInsights.co_changes` is computed by git-intelligence analysis, and `FileCard.co_changes` is a `Vec<FileRef>` field that has never been populated. The pipeline computes co-change data but never persists it as graph edges, so the card surface always returns an empty list. The existing specs already describe the intended behavior; this change closes the implementation gap.

## What Changes

- Emit `CoChangesWith` edges between file nodes during stage 5 (git mining), using the already-computed `GitHistoryInsights.co_changes` data with `Epistemic::GitObserved` authority.
- Apply a minimum co-change count threshold (defaulting to 2) to avoid low-signal noise from single co-occurrences.
- Wire `FileCard.co_changes` to read from persisted graph edges instead of returning `vec![]`.
- Handle edge lifecycle: re-emitting edges on reconcile, removing stale edges when co-change counts drop below threshold or files disappear.

## Capabilities

### New Capabilities

None. The graph, git-intelligence, and cards specs already describe co-change edge behavior.

### Modified Capabilities

None at the spec level. All three specs already permit or require this behavior. The change is a pure implementation gap closure.

## Impact

- `src/pipeline/structural/` or `src/pipeline/git_intelligence/`: new edge emission step after analysis.
- `src/store/sqlite/`: edge persistence (uses existing `upsert_edge` / `remove_edge` paths).
- `src/surface/card/compiler/file.rs`: populate `co_changes` from graph edges instead of hardcoding `vec![]`.
- Snapshot tests in `src/surface/card/compiler/` will update when co-change data appears.
- Reconcile and watch paths must re-emit co-change edges on each pass.
