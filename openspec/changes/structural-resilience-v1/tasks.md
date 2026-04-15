## 1. Drift score storage

- [ ] 1.1 Add `edge_drift` table DDL to `src/store/sqlite/schema.rs` (edge_id BLOB, revision TEXT, drift_score REAL, composite PK, WITHOUT ROWID)
- [ ] 1.2 Add `write_drift_scores(edge_scores: &[(EdgeId, f32)], revision: &str)` and `read_drift_scores(revision: &str)` methods to `SqliteGraphStore`
- [ ] 1.3 Add `truncate_drift_scores(older_than_revision: &str)` method to `SqliteGraphStore`
- [ ] 1.4 Add drift score methods to the `GraphStore` trait (with default no-op impls) so the pipeline can call them through the trait

## 2. Structural fingerprint

- [ ] 2.1 Define `StructuralFingerprint` type in `src/structure/drift.rs` as a sorted set of `(qualified_name: String, signature_hash: u64)` pairs
- [ ] 2.2 Implement `fingerprint_for_file(file_node: &FileNode, graph: &dyn GraphStore) -> StructuralFingerprint` that collects all symbol qualified names and their signature hashes from the graph
- [ ] 2.3 Implement `Jaccard distance between two fingerprints` as a method on `StructuralFingerprint`
- [ ] 2.4 Add unit tests for fingerprint comparison: identical files, files with added symbols, files with removed symbols, files with changed signatures

## 3. Stage 7: drift scoring

- [ ] 3.1 Implement `compute_drift_score(edge: &Edge, graph: &dyn GraphStore, previous_fingerprints: &HashMap<FileNodeId, StructuralFingerprint>) -> f32` in `src/structure/drift.rs` using the scoring bands from the existing doc comment
- [ ] 3.2 Add `run_drift_scoring(graph: &mut dyn GraphStore, revision: &str)` function that iterates all edges, computes drift, and writes scores to the sidecar table
- [ ] 3.3 Wire `run_drift_scoring` into the structural compile pipeline after stages 1-4 (in `src/pipeline/structural/mod.rs` or `stages.rs`), making it stage 7
- [ ] 3.4 Add integration test: create a graph with edges, modify symbol signatures, re-compile, verify drift scores are non-zero for changed edges and zero for unchanged edges
- [ ] 3.5 Verify drift 1.0 for edges pointing to deleted artifacts

## 4. Stage 6: split and merge identity cascade

- [ ] 4.1 Implement `symbol_set_for_file(file_node: &FileNode, graph: &dyn GraphStore) -> HashSet<String>` helper in `src/structure/identity.rs` that collects qualified symbol names
- [ ] 4.2 Implement `detect_split(disappeared: &FileNode, new_files: &[FileNode], graph: &dyn GraphStore) -> Option<IdentityResolution>` with Jaccard threshold 0.4
- [ ] 4.3 Implement `detect_merge(disappeared: &[FileNode], new_files: &[FileNode], graph: &dyn GraphStore) -> Vec<IdentityResolution>` with Jaccard threshold 0.5
- [ ] 4.4 Wire `resolve_identities()` to run the full cascade: content-hash rename (existing), split, merge, git rename, breakage
- [ ] 4.5 Add function to persist identity resolutions: write `SplitFrom`/`MergedFrom` edges to graph, update `path_history`, mark superseded nodes
- [ ] 4.6 Wire identity cascade into structural compile pipeline as stage 6 (before drift scoring)
- [ ] 4.7 Add integration test: parse a repo, simulate a file split (modify source files, re-compile), verify `SplitFrom` edge exists and old symbols are not orphaned
- [ ] 4.8 Add integration test: simulate a file merge, verify `MergedFrom` edges and superseded markers
- [ ] 4.9 Add integration test: no-match case produces `Breakage` with logged reason

## 5. Repair loop integration

- [ ] 5.1 Add `RepairSurface::EdgeDrift` variant to `src/pipeline/repair/types/stable.rs` (update both serde rename and `as_str()`)
- [ ] 5.2 Add `DriftClass::HighDriftEdge` variant alongside existing drift classes
- [ ] 5.3 Implement edge-drift drift detection in `src/pipeline/repair/report.rs`: query drift scores, report edges >= 0.7, report edges at 1.0 as requiring pruning
- [ ] 5.4 Implement edge pruning in `src/pipeline/repair/sync.rs`: delete edges at drift 1.0, log each pruning action
- [ ] 5.5 Update repair types tests to cover new `EdgeDrift` surface and `HighDriftEdge` drift class
- [ ] 5.6 Add integration test: create edges, set drift to 1.0, run `check`, verify drift surface reported; run `sync`, verify edges pruned and logged

## 6. Verification and cleanup

- [ ] 6.1 Run `make check` (fmt, clippy, all tests) and verify clean exit
- [ ] 6.2 Run `cargo test -p synrepo identity` and `cargo test -p synrepo drift` to verify stage 6 and 7 tests pass
- [ ] 6.3 Verify `synrepo init` on a test repo produces no errors with stages 6 and 7 wired
- [ ] 6.4 Update `ROADMAP.md`: mark stages 6 and 7 as implemented in Track D
- [ ] 6.5 Run `openspec validate --change structural-resilience-v1` to verify change artifacts
