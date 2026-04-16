# runtime-safety-hardening-v1 tasks

## 1. Writer ownership model

- [x] 1.1 Change writer re-entrancy from process-scoped path depth to
      thread-scoped or explicit-guard re-entry.
- [x] 1.2 Reject concurrent same-process acquisition from a different thread.
- [x] 1.3 Add a single write-admission helper that:
      - checks watch ownership
      - delegates when watch is authoritative
      - otherwise acquires the writer lock
- [x] 1.4 Move mutating CLI paths to the shared write-admission helper.
- [x] 1.5 Review long write critical sections and move read/plan work outside
      the lock where safe.

## 2. Lock diagnostics and recovery

- [x] 2.1 Preserve writer ownership diagnostics for status output.
- [x] 2.2 Keep stale-lock recovery behavior.
- [x] 2.3 Add explicit error variants for:
      - watch-owned repo
      - same-process wrong-thread re-entry
      - malformed ownership record
- [x] 2.4 Ensure crash-recovery flows still work for overlay promotion paths.

## 3. Vector query memory behavior

- [x] 3.1 Replace full `Vec<(index, score)>` allocation with bounded top-k
      selection.
- [x] 3.2 Stop sorting the full corpus when only top-k is needed.
- [x] 3.3 Precompute or persist vector normalization data if query semantics
      remain cosine-based.
- [x] 3.4 Add benchmarks or tests covering large-index query behavior.

## 4. Overlay and status memory behavior

- [x] 4.1 Push review queue sort/limit into the overlay store query layer.
- [x] 4.2 Avoid loading all candidates when the CLI only needs a limited page.
- [x] 4.3 Reduce full-materialization scans in status/report surfaces where
      aggregate counts are sufficient.
- [x] 4.4 Add regression tests that prove limits are applied before full
      result materialization when supported by the store.

## 5. Tests

- [x] 5.1 Add a test proving nested same-thread re-entry is allowed.
- [x] 5.2 Add a test proving different-thread same-process acquisition is
      rejected.
- [x] 5.3 Add a test proving watch-mediated write admission blocks or delegates
      consistently.
- [x] 5.4 Add a vector-query test that validates bounded top-k behavior without
      full score sorting semantics changing.
- [x] 5.5 Add store/query tests for SQL-side limit/order behavior.