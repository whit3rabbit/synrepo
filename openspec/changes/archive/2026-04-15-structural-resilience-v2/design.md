## Context

The v1 design specified cross-revision fingerprint comparison, all-edge scoring, concept-edge handling, and a five-step identity cascade. The implementation shipped a simpler heuristic for drift, skipped concept edges, and stubbed the git rename fallback. This design fills those gaps by reusing the infrastructure v1 already put in place.

## Decisions

### D1: Prior fingerprint storage

**Decision**: Add a `file_fingerprints` sidecar table keyed by `(file_node_id, revision)`. Write current fingerprints at the end of each drift scoring pass. Read the prior revision's fingerprints at the start of the next pass.

**Rationale**: The `edge_drift` table already establishes the pattern of revision-keyed sidecar storage. A separate fingerprint table avoids coupling fingerprint persistence to drift scoring output (fingerprints are useful for diagnostics and future card payloads independently).

**Schema**:
```sql
CREATE TABLE IF NOT EXISTS file_fingerprints (
    file_node_id INTEGER NOT NULL,
    revision TEXT NOT NULL,
    fingerprint BLOB NOT NULL,
    PRIMARY KEY (file_node_id, revision)
) WITHOUT ROWID;
```

The fingerprint is the serialized `BTreeSet<FingerprintEntry>` via bincode. This is cheap to write and read, and the set is small (tens to hundreds of entries per file).

### D2: Drift score computation with Jaccard distance

**Decision**: For each edge, resolve both endpoints to their file-level fingerprints (prior and current). Compute `prior_fp.jaccard_distance(&current_fp)` for each endpoint. The edge's drift score is the average of both endpoints' distances. Deleted endpoints (file absent from current fingerprint map) score 1.0.

**Rationale**: This is what the v1 design specified and what `StructuralFingerprint::jaccard_distance()` was built for. The heuristic (empty/non-empty) cannot distinguish between "file gained a symbol" and "file lost all symbols," both of which have different drift implications.

**Scoring bands** (unchanged from v1 intent):
- 0.0: identical fingerprints
- 0.0-0.3: minor divergence (added/removed symbols)
- 0.3-0.7: moderate divergence (signature changes, significant symbol turnover)
- 0.7-1.0: high divergence (major structural change)
- 1.0: endpoint deleted

### D3: All-edge enumeration

**Decision**: Add `GraphStore::all_edges()` that returns every edge regardless of source node type. Use this as the drift scoring worklist instead of file-outbound traversal.

**Rationale**: File-outbound traversal misses edges where a symbol or concept node is the source. The cost of a full scan is bounded by the number of edges (typically tens of thousands), and the scoring pass already touches every edge.

### D4: Concept-edge drift

**Decision**: For edges where one endpoint is a concept node, resolve the non-concept endpoint's file fingerprint and score against that. For concept-to-concept edges, score 0.0 (no structural fingerprint to compare). If the non-concept endpoint's file is deleted, score 1.0.

**Rationale**: The primary use case for concept-edge drift is detecting prose rot: a concept document (ADR, decision) references a code artifact that has since changed structurally. The concept node itself has no structural fingerprint, so the drift signal comes entirely from the code side.

### D5: Git rename fallback via gix

**Decision**: Add a `detect_renames()` helper in `src/pipeline/git/` that uses `gix::diff::tree()` with rewrite tracking enabled. This produces (old_path, new_path) pairs that the identity cascade consumes as step 4.

**Rationale**: The current git history collector disables rewrite tracking for performance. Rename detection needs it enabled but only runs when content-hash, split, and merge checks all fail, which is a rare path. Using a separate function avoids slowing down the common case.

**Constraint**: `gix` is already a dependency. No new crates needed.

### D6: Repair absent-vs-current

**Decision**: `edge_drift_findings()` should return `DriftClass::Absent` when the graph exists but the `edge_drift` table has zero rows for any revision. This means no structural compile has ever written drift scores. Return `DriftClass::Current` only when drift rows exist and all are below 0.7.

**Rationale**: The current code conflates "no data" with "all good." A fresh `synrepo init` followed by `synrepo check` should not claim edge drift is healthy before any drift scoring has run.

## Risks

- **Fingerprint storage growth**: One row per file per revision. Mitigated by truncating old revisions at the start of each cycle (same pattern as `edge_drift`).
- **All-edge scan cost**: Linear in edge count. Acceptable for typical repo sizes (< 100k edges). Can be optimized later with incremental scoring if needed.
- **Git rename false positives**: gix rename detection uses heuristics (similarity thresholds). May produce false matches. Mitigated by running only after three more precise checks have failed.

## Files affected

- `src/structure/drift.rs` -- rewrite `compute_drift_score()`, update `run_drift_scoring()`
- `src/structure/graph/store.rs` -- add `all_edges()`, fingerprint read/write to `GraphStore` trait
- `src/structure/graph/edge.rs` -- update `drift_score` doc comment
- `src/structure/identity.rs` -- wire git rename step, update `persist_resolutions()`
- `src/store/sqlite/schema.rs` -- add `file_fingerprints` table
- `src/store/sqlite/ops.rs` -- fingerprint and all-edges read/write
- `src/store/sqlite/mod.rs` -- wire new methods
- `src/pipeline/structural/mod.rs` -- pass prior revision to drift scoring
- `src/pipeline/structural/stages.rs` -- pass git repo to identity cascade
- `src/pipeline/git/mod.rs` -- add `detect_renames()`
- `src/pipeline/repair/report.rs` -- fix absent-vs-current in `edge_drift_findings()`
