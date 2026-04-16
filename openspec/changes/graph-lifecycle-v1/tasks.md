## 1. Schema and types (additive, non-breaking)

- [x] 1.1 Add `compile_revisions` table DDL to `src/store/sqlite/schema.rs` (revision_id INTEGER PK, created_at TEXT, file_count INTEGER, symbol_count INTEGER)
- [x] 1.2 Add `last_observed_rev INTEGER` column to `files` table via `ALTER TABLE ADD COLUMN` in migration path
- [x] 1.3 Add `last_observed_rev INTEGER`, `retired_at_rev INTEGER` columns to `symbols` table
- [x] 1.4 Add `owner_file_id INTEGER`, `last_observed_rev INTEGER`, `retired_at_rev INTEGER` columns to `edges` table
- [x] 1.5 Add `last_observed_rev INTEGER` column to `concepts` table
- [x] 1.6 Add `last_observed_rev: Option<u64>` field to `FileNode` in `src/structure/graph/node.rs`
- [x] 1.7 Add `last_observed_rev: Option<u64>`, `retired_at_rev: Option<u64>` fields to `SymbolNode`
- [x] 1.8 Add `owner_file_id: Option<FileNodeId>`, `last_observed_rev: Option<u64>`, `retired_at_rev: Option<u64>` fields to `Edge` in `src/structure/graph/edge.rs`
- [x] 1.9 Add `last_observed_rev: Option<u64>` field to `ConceptNode`
- [x] 1.10 Add `retain_retired_revisions: u64` config field to `src/config.rs` (default 10)
- [x] 1.11 Bump store format version in `src/store/compatibility/`; add migration evaluator entry that runs ALTER TABLE + backfill
- [x] 1.12 Backfill migration: insert initial `compile_revisions` row (rev 1), set `last_observed_rev = 1` for all existing rows, resolve `owner_file_id` on edges via from-endpoint join
- [x] 1.13 Unit test: migration roundtrip on a pre-migration fixture store

## 2. GraphStore trait extensions

- [x] 2.1 Add `next_compile_revision(&mut self) -> Result<u64>` to `GraphStore` trait and `SqliteGraphStore`
- [x] 2.2 Add `retire_symbol(&mut self, id: SymbolNodeId, revision: u64)` and `retire_edge(&mut self, id: EdgeId, revision: u64)`
- [x] 2.3 Add `unretire_symbol` and `unretire_edge` (clear `retired_at_rev`, set `last_observed_rev`)
- [x] 2.4 Add `symbols_for_file(file_id: FileNodeId) -> Vec<SymbolNode>` (active only, `retired_at_rev IS NULL`)
- [x] 2.5 Add `edges_owned_by(file_id: FileNodeId) -> Vec<Edge>` (active only)
- [x] 2.6 Add `active_edges() -> Vec<Edge>` (`retired_at_rev IS NULL`)
- [x] 2.7 Update `upsert_file`, `upsert_symbol`, `insert_edge` to write observation-window fields when present
- [x] 2.8 Unit tests: retire/unretire roundtrip, symbols_for_file, edges_owned_by, active_edges vs all_edges

## 3. Replace destructive rebuild with scoped refresh

- [x] 3.1 Add `refresh_file_observations(graph, file_id, new_content_hash, new_symbols, new_edges, revision)` helper in `src/pipeline/structural/stages.rs`
- [x] 3.2 Remove `graph.delete_node(NodeId::File(...))` from the content-change branch (around line 176)
- [x] 3.3 Wire `refresh_file_observations` into the content-change branch: upsert file, diff symbols, diff edges, retire/insert as needed
- [x] 3.4 Pass compile revision through `stages_1_to_3` and `process_supported_code_files`
- [x] 3.5 Call `next_compile_revision` at the start of `run_structural_compile` in `src/pipeline/structural/mod.rs`
- [x] 3.6 Set `owner_file_id` on edges emitted by stage 4 cross-file resolution
- [x] 3.7 Integration test: edit a file in place, verify `FileNodeId` persists, old symbols retired, new symbols inserted
- [x] 3.8 Integration test: delete a file, verify `delete_node` still cascades (file genuinely gone)
- [x] 3.9 Integration test: add a new file, verify `last_observed_rev` set on all emitted nodes/edges

## 4. Drift uses observation windows

- [x] 4.1 Verify `all_edges()` returns retired edges (should already work since it reads all rows)
- [x] 4.2 Verify `compute_drift_score` handles retired edges correctly (endpoint file still exists, fingerprint comparison works)
- [x] 4.3 Add test: content-edit produces non-zero drift on owning edges, file_id unchanged
- [x] 4.4 Add test: retired edge whose endpoint file is gone scores 1.0
- [x] 4.5 Update card compilation and MCP queries to use `active_edges()` instead of `all_edges()` where they should not see retired observations

## 5. Compaction maintenance pass

- [x] 5.1 Implement `compact_retired(older_than_rev: u64)` in `SqliteGraphStore`: delete retired symbols, edges, old compile_revisions, old fingerprints, old edge_drift rows
- [x] 5.2 Add `CompactionSummary` struct (symbols_removed, edges_removed, revisions_removed)
- [x] 5.3 Wire compaction into `synrepo sync` after repair actions complete
- [x] 5.4 Wire compaction into `synrepo upgrade --apply`
- [x] 5.5 Read `retain_retired_revisions` from config to compute the compaction threshold
- [x] 5.6 Unit test: retired observations older than threshold are physically deleted
- [x] 5.7 Unit test: retired observations within threshold survive compaction
- [x] 5.8 Unit test: active observations are never deleted by compaction

## 6. Repair surface for retired observations

- [x] 6.1 Add `RepairSurface::RetiredObservations` variant in `src/pipeline/repair/types/stable.rs` (serde rename + `as_str()` dual mapping)
- [x] 6.2 Add `RepairAction::CompactRetired` variant (serde + `as_str()`)
- [ ] 6.3 Add `retired_observations_findings()` to `src/pipeline/repair/report.rs`: count retired symbols/edges, report compaction recommendation when count exceeds threshold
- [ ] 6.4 Wire `retired_observations_findings` into `check` output
- [ ] 6.5 Wire `CompactRetired` action into `sync` dispatch
- [x] 6.6 Update stable-identifier tests in `src/pipeline/repair/types/tests.rs`

## 7. End-to-end verification

- [x] 7.1 `make check` passes clean (fmt, clippy, all tests)
- [ ] 7.2 Manual: `cargo run -- init` on fixture repo, edit a file, re-init, verify `FileNodeId` persists via `synrepo node <id>`
- [ ] 7.3 Manual: `synrepo check --json` after edit shows non-zero drift, file_id unchanged
- [ ] 7.4 Manual: delete a symbol, verify `retired_at_rev` set, `active_edges()` excludes its edges
- [ ] 7.5 Update CLAUDE.md/AGENTS.md: document new invariant (FileNodeId stable across content edits), new config field, compaction behavior
- [ ] 7.6 Update ROADMAP.md: add graph-lifecycle-v1 entry
