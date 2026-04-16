## Context

`structural-resilience-v1` shipped the infrastructure for stages 6 and 7: types, sidecar table, pipeline wiring, repair surface, and split/merge detection. The implementations cut corners relative to the design:

1. **Drift scoring uses an empty/non-empty heuristic** instead of cross-revision Jaccard distance on structural fingerprints. `StructuralFingerprint::jaccard_distance()` exists but `compute_drift_score()` never calls it. Prior fingerprints are not persisted, so "then vs now" comparison is structurally impossible.
2. **Concept edges always score 0.0**, defeating the prose-rot detection use case from the v1 proposal.
3. **Edge worklist is file-outbound only**, so symbol-to-symbol and concept-involved edges are unreachable by the scoring pass.
4. **Git rename fallback (cascade step 4) is a stub** that logs "Not yet wired; future work" and falls through to breakage.
5. **Repair returns `Current` when no drift rows exist**, when it should say `Absent` (no assessment performed yet).

This change finishes the work v1 started.

## Goals

- Replace the heuristic drift scoring with Jaccard distance on persisted prior-vs-current structural fingerprints.
- Score every graph edge, including concept-involved edges.
- Wire the git rename fallback in stage 6 step 4.
- Fix the repair absent-vs-current distinction.
- Add integration tests that lock down the corrected semantics.

## Non-Goals

- Drift-driven card payload changes (future `ChangeRiskCard`).
- Symbol-level identity resolution (file-level only, as in v1).
- ArcSwap commit (stage 8, separate change).
- Split/merge detection changes (already correct in v1).
