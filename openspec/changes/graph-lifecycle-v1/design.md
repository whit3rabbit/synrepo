## Context

The structural compile currently calls `delete_node(NodeId::File(...))` on any content-hash change. The cascade wipes every symbol and every incident edge for the file, then re-inserts everything from scratch. Drift scoring (shipped in `structural-resilience-v2`) assumes stable `FileNodeId` and persisted fingerprints across revisions. The destructive model erases the continuity that drift, repair, and provenance surfaces need.

This design separates identity from observation, introduces ownership and soft retirement, and defers physical deletion to a maintenance pass.

## Invariants

1. **Stable file identity across content edits.** `FileNodeId` is never invalidated by a content-hash change. `content_hash` is a version field on `FileNode`, not a deletion trigger. `delete_node(NodeId::File(...))` is reserved for files genuinely absent from the repo and for compaction.

2. **Observation ownership.** Every parser-observed symbol and edge records `owner_file_id`: the `FileNodeId` whose parse pass produced it. Recompiling file F retires observations owned by F that were not re-emitted, without touching observations owned by other files.

3. **Soft retirement on non-observation.** A symbol or edge not re-emitted at compile revision R is marked `retired_at_rev = R`. It remains physically present until compaction. An edge or symbol re-emitted at a later revision clears `retired_at_rev`.

4. **Physical deletion scope.** Physical deletion happens in exactly three cases: (a) the source file genuinely disappeared from the repo and the identity cascade found no rename/split/merge match, (b) the compaction maintenance pass removes observations retired longer than `retain_retired_revisions` revisions ago, (c) explicit human override.

5. **Epistemic ownership boundary.** Human-declared facts (`Epistemic::HumanDeclared`, `EdgeKind::Governs`) are never retired by a parser pass. Only the emitter class that produced a fact can retire it. Parser passes retire `ParserObserved` observations only.

6. **Drift reads observation windows.** `run_drift_scoring` must see edges retired at the current revision so it can score the transition from observed to absent. Callers that want active-only edges use a filter flag.

## Decisions

### D1: Compile revision counter

**Decision**: Add a `compile_revisions` table with a monotonically increasing `revision_id` (integer), a `created_at` timestamp, and summary counts. Each structural compile increments the counter. The revision id is the observation-window coordinate for all ownership and retirement fields.

**Rationale**: Using a synthetic counter rather than a git commit SHA avoids coupling the compile lifecycle to git state. Multiple compiles can happen within one git commit (reconcile loop, manual re-init). The counter is cheap and deterministic.

**Schema**:
```sql
CREATE TABLE IF NOT EXISTS compile_revisions (
    revision_id INTEGER PRIMARY KEY,
    created_at  TEXT NOT NULL,
    file_count  INTEGER NOT NULL DEFAULT 0,
    symbol_count INTEGER NOT NULL DEFAULT 0
);
```

### D2: Observation-window fields on nodes and edges

**Decision**: Add the following nullable columns to core tables. All are `Option<u64>` in Rust, defaulting to `None` for pre-migration rows.

| Table | New columns |
|-------|------------|
| `files` | `last_observed_rev INTEGER` |
| `symbols` | `last_observed_rev INTEGER`, `retired_at_rev INTEGER` |
| `edges` | `owner_file_id INTEGER`, `last_observed_rev INTEGER`, `retired_at_rev INTEGER` |
| `concepts` | `last_observed_rev INTEGER` |

**Rationale**: Nullable columns with `ALTER TABLE ... ADD COLUMN` are a non-breaking schema migration in SQLite (no table rebuild needed). Existing stores upgrade in place. The compatibility layer bumps the format version and backfills `last_observed_rev` to the current revision for all active rows during migration.

### D3: Scoped file refresh replaces destructive rebuild

**Decision**: Replace the content-change branch in `stages.rs` (currently `delete_node` + `upsert_file`) with a `refresh_file_observations` helper that:

1. Upserts `FileNode` with new `content_hash` and `last_observed_rev = current_rev`. Identity is preserved.
2. Loads prior symbols owned by this file via `symbols_for_file(file_id)`.
3. Diffs prior symbols against newly parsed symbols by `(qualified_name, kind)`.
   - Match found: upsert with new `body_hash`, `signature`, `body_byte_range`, `last_observed_rev`.
   - No match in new set: set `retired_at_rev = current_rev`.
   - New symbol not in prior set: insert with `last_observed_rev = current_rev`.
4. Loads prior edges owned by this file via `edges_owned_by(file_id)`.
5. Diffs prior edges against newly emitted edges by `(from, to, kind)`.
   - Match found: upsert with `last_observed_rev = current_rev`.
   - No match in new set: set `retired_at_rev = current_rev` (parser-observed only; skip `HumanDeclared`).
   - New edge not in prior set: insert with `owner_file_id`, `last_observed_rev = current_rev`.

**Rationale**: This preserves `FileNodeId` stability, enables drift scoring on pre/post observation windows, and avoids orphaned graph state because every observation is scoped to its owner file.

### D4: GraphStore trait extensions

**Decision**: Add the following methods to the `GraphStore` trait:

```rust
fn next_compile_revision(&mut self) -> Result<u64>;
fn retire_symbol(&mut self, id: SymbolNodeId, revision: u64) -> Result<()>;
fn retire_edge(&mut self, id: EdgeId, revision: u64) -> Result<()>;
fn unretire_symbol(&mut self, id: SymbolNodeId, revision: u64) -> Result<()>;
fn unretire_edge(&mut self, id: EdgeId, revision: u64) -> Result<()>;
fn symbols_for_file(&self, file_id: FileNodeId) -> Result<Vec<SymbolNode>>;
fn edges_owned_by(&self, file_id: FileNodeId) -> Result<Vec<Edge>>;
fn active_edges(&self) -> Result<Vec<Edge>>;
fn compact_retired(&mut self, older_than_rev: u64) -> Result<CompactionSummary>;
```

`all_edges()` (existing) continues to return all edges including retired. `active_edges()` returns only non-retired edges for card compilation and MCP queries. Drift scoring uses `all_edges()`.

**Rationale**: Keeping `all_edges()` inclusive and adding `active_edges()` as the filtered variant avoids breaking existing callers while giving new callers the semantics they need.

### D5: Drift scoring with observation windows

**Decision**: `compute_drift_score` continues to use Jaccard distance on structural fingerprints, unchanged from v2. The only change is that `run_drift_scoring` feeds it edges from `all_edges()` which now includes retired-this-revision edges. A retired edge whose endpoint file still exists gets scored normally (fingerprint change). A retired edge whose endpoint file is genuinely absent (deleted from repo) scores 1.0 as before.

**Rationale**: The drift math is correct. The problem was the input: post-deletion, the edges that should be scored were already gone. With soft retirement, they survive long enough for drift to assess them.

### D6: Compaction

**Decision**: Add `compact_retired(older_than_rev: u64)` that physically deletes:
- Symbols where `retired_at_rev IS NOT NULL AND retired_at_rev < older_than_rev`
- Edges where `retired_at_rev IS NOT NULL AND retired_at_rev < older_than_rev`
- Old `compile_revisions` rows below the retention window
- Old `file_fingerprints` and `edge_drift` rows below the retention window

Default retention: 10 revisions (`retain_retired_revisions` config field). Compaction runs during `synrepo sync` and `synrepo upgrade --apply`, never during the hot reconcile path.

**Rationale**: Unbounded accumulation of retired observations would bloat the store. 10 revisions gives enough history for drift scoring and repair narration while keeping the store compact.

### D7: Compatibility migration

**Decision**: Bump store format version. The migration evaluator:
1. Adds the new columns via `ALTER TABLE ... ADD COLUMN` (no table rebuild).
2. Inserts an initial `compile_revisions` row (revision 1).
3. Backfills `last_observed_rev = 1` for all existing rows (they are assumed active).
4. Sets `owner_file_id` on existing edges by resolving the `from` endpoint: if `from` is a symbol, use that symbol's `file_id`; if `from` is a file, use that file's id; if `from` is a concept, set to NULL (concepts are not file-owned).

**Rationale**: This makes pre-migration stores usable without a full re-init. The backfill is a conservative default (everything is active at revision 1). Users who want a clean state can run `synrepo init` to rebuild.

## Risks

- **Migration correctness**: The `owner_file_id` backfill for edges whose `from` is a symbol requires a join. If the join produces no match (orphaned edge), the edge gets `owner_file_id = NULL` and will not be retired by any file refresh, only by compaction. This is acceptable as a degraded-but-safe default.
- **Symbol matching by `(qualified_name, kind)`**: If a rename changes `qualified_name`, the old symbol is retired and a new one inserted. This is correct behavior (the identity changed), but it means the `SymbolNodeId` changes. The existing `SymbolNodeId` derivation already includes `qualified_name`, so this is consistent.
- **Performance**: `symbols_for_file` and `edges_owned_by` add two queries per changed file. Both are indexed lookups. For typical repos (hundreds of changed files per reconcile), this is negligible.

## Files affected

- `src/structure/graph/node.rs` -- add `last_observed_rev`, `retired_at_rev` fields
- `src/structure/graph/edge.rs` -- add `owner_file_id`, `last_observed_rev`, `retired_at_rev` fields
- `src/structure/graph/store.rs` -- add trait methods (D4)
- `src/store/sqlite/schema.rs` -- add columns, `compile_revisions` table
- `src/store/sqlite/ops.rs` -- implement new trait methods, retirement/compaction logic
- `src/store/compatibility/` -- version bump, migration evaluator entry
- `src/pipeline/structural/stages.rs` -- replace content-change delete with `refresh_file_observations`
- `src/pipeline/structural/mod.rs` -- pass compile revision through pipeline
- `src/structure/drift.rs` -- no math changes; verify `all_edges()` inclusion of retired edges
- `src/pipeline/repair/report.rs` -- add retired-observation surface
- `src/pipeline/repair/types/stable.rs` -- add `RetiredObservations` variant (serde + `as_str()`)
- `src/pipeline/maintenance.rs` -- wire compaction into sync/upgrade
- `src/config.rs` -- add `retain_retired_revisions` config field
