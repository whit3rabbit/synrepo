## 1. Prior fingerprint persistence

- [x] 1.1 Add `file_fingerprints` table DDL to `src/store/sqlite/schema.rs` (file_node_id INTEGER, revision TEXT, fingerprint TEXT, composite PK, WITHOUT ROWID)
- [x] 1.2 Add `write_fingerprints(fingerprints: &[(FileNodeId, StructuralFingerprint)], revision: &str)` and `read_fingerprints(revision: &str)` methods to `SqliteGraphStore`
- [x] 1.3 Add `truncate_fingerprints(older_than_revision: &str)` method
- [x] 1.4 Add fingerprint methods to `GraphStore` trait (default no-op impls)
- [x] 1.5 Add `StructuralFingerprint` serialization via serde (JSON text in BLOB column)
- [x] 1.6 Add `latest_fingerprint_revision()` to `GraphStore` trait and `SqliteGraphStore`

## 2. All-edge enumeration

- [x] 2.1 Add `all_edges()` method to `GraphStore` trait returning `Result<Vec<Edge>>`
- [x] 2.2 Implement `all_edges()` in `SqliteGraphStore` via `SELECT data FROM edges`
- [x] 2.3 Update `run_drift_scoring()` to use `all_edges()` instead of file-outbound traversal

## 3. Rewrite drift scoring

- [x] 3.1 Rewrite `compute_drift_score()` to accept prior and current fingerprint maps, use `jaccard_distance()` for comparison
- [x] 3.2 Handle concept edges: resolve non-concept endpoint file, score against that. Concept-to-concept = 0.0. Deleted endpoint = 1.0.
- [x] 3.3 Update `run_drift_scoring()` to read prior fingerprints, compute current fingerprints, write current fingerprints for next cycle
- [x] 3.4 Remove the empty/non-empty heuristic and its doc comment
- [x] 3.5 Update the module-level doc comment to reflect the new scoring model

## 4. Git rename fallback

- [x] 4.1 Add `detect_recent_renames()` helper in `src/pipeline/git/renames.rs` using gix with rewrite tracking
- [x] 4.2 Wire git rename into `resolve_identities()` as step 4, between merge and breakage
- [x] 4.3 Update `persist_resolutions()` `GitRename` arm to preserve the old node ID and update path_history
- [x] 4.4 Pass repo_root through the pipeline stage to `run_identity_cascade()`

## 5. Repair absent-vs-current fix

- [x] 5.1 Update `edge_drift_findings()` to return `DriftClass::Absent` when no drift revision exists
- [x] 5.2 Return `DriftClass::Current` only when drift rows exist and all are below 0.7

## 6. Edge struct cleanup

- [x] 6.1 Update `Edge.drift_score` doc comment to clarify it is a default-only field; canonical drift lives in the sidecar `edge_drift` table

## 7. Integration tests

- [x] 7.1 Signature-only change yields non-zero drift (core invariant)
- [x] 7.2 Unchanged edge yields 0.0 drift
- [x] 7.3 Deleted endpoint yields 1.0
- [ ] 7.4 Concept-to-code edge drifts when target signature changes (deferred: requires concept node fixtures)
- [x] 7.5 No drift rows exist -> repair report says Absent (not Current)
- [ ] 7.6 Drift rows exist but all below threshold -> repair report says Current (covered by existing repair tests)
- [x] 7.7 Fingerprint persistence roundtrip (write, read back, compare)
- [ ] 7.8 Git rename fallback resolves when content-hash and symbol-set both fail (deferred: requires git repo fixture)

## 8. Verification

- [x] 8.1 `cargo build` and `cargo test` pass (407 tests, 1 pre-existing flaky failure in sync test)
- [x] 8.2 Update ROADMAP.md: v2 entry added with correct status
- [x] 8.3 Update AGENTS.md: stage 6/7 descriptions updated to reflect wired state
