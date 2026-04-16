# runtime-safety-hardening-v1 design

## Summary

This change makes runtime mutation admission explicit and narrows write
exclusivity to the actual unit of execution rather than the entire process.
It also reduces memory amplification in query and reporting paths.

## Writer model

### Current weakness

The current writer model uses a lock file plus a per-lock-path re-entrancy map.
That permits nested acquisition in the same process, but it does not cleanly
separate:

- legitimate nested re-entry in one call chain
- accidental concurrent mutation from another thread in the same process

The result is that "single writer" is weaker than it appears.

### New rule

Writer ownership is granted to exactly one active write execution context.

Allowed:
- nested acquisition in the same thread or through an explicit write token

Rejected:
- acquisition from another thread in the same process
- acquisition from another live process
- acquisition when watch mode is authoritative for the repo and delegation is
  required

### Admission flow

All mutating operations enter through a shared helper:

1. Inspect watch state.
2. If watch is authoritative and supports delegation for the operation, delegate.
3. If watch is authoritative and delegation is not supported, fail clearly.
4. Otherwise acquire writer ownership.
5. Run the mutation.
6. Release ownership.

This removes duplicated "ensure watch not running" plus "acquire writer lock"
logic from individual commands.

## Critical-section duration

Lock hold time should cover commit phases, not all compute phases, where safe.

Examples:
- planning compaction can happen before lock acquisition
- deterministic scan/report building can happen before lock acquisition
- final state mutation, file replacement, and cross-store commit sequences stay
  under the lock

Cross-store sequences that rely on recovery markers, such as overlay pending ->
graph insert -> overlay promoted, remain fully protected by write ownership.

## Memory behavior

### Vector query

Current brute-force vector query is acceptable, but the query path should not
allocate and sort a score entry for every chunk when only top-k is needed.

New approach:
- scan all vectors
- maintain a bounded min-heap of size k
- return only the retained winners
- preserve current ranking semantics

If cosine similarity remains the metric, normalization data may be precomputed
or vectors may be stored normalized so repeated chunk-norm computation is
avoided.

### Overlay review and list surfaces

When the user requests a limited review/list view, the storage layer should
apply sort and limit rather than Rust loading all candidates and truncating
afterward.

### Status/report aggregation

Status/report paths should prefer aggregate queries over full row scans when the
surface only needs counts or freshness summaries.

## Migration and compatibility

This change does not alter persisted graph semantics.

If vector normalization metadata is added to the embedding index, the index
format version must be bumped and old indices must be rebuilt or rejected
cleanly.

## Risks

- tightening re-entrancy may break tests or hidden nested write paths that
  currently rely on process-wide permissiveness
- reducing critical-section size must not reintroduce atomicity gaps
- SQL-side limit/order changes must preserve existing result ordering semantics