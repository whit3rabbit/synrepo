## Context

The `symbols` table is a TEXT-PK `WITHOUT ROWID` store. Every field except `id`, `file_id`, `qualified_name`, and `kind` lives inside the `data` TEXT column as JSON. Older additions (`first_seen_rev`, `last_modified_rev`, `last_observed_rev`, `retired_at_rev`) were promoted out of the JSON into dedicated columns via the `migratables` vector in `src/store/sqlite/schema.rs:71-133`, which ADD COLUMNs idempotently and is the established pattern.

`body_hash` is a 64-hex string (blake3 of the symbol body). It is load-bearing for symbol identity (`SymbolNodeId` keys on `(file_id, qualified_name, kind, body_hash)` per `src/core/ids.rs:59`). The current JSON-blob-only storage forces `json_extract` on every read in `all_symbols_summary`.

Two implementation options were considered: a regular TEXT column populated by insert-path updates, or a STORED generated column computed from `data` via `GENERATED ALWAYS AS (json_extract(data, '$.body_hash')) STORED`. The latter avoids touching insert paths but has SQLite-version and `WITHOUT ROWID` compatibility quirks that are not worth chasing for a one-off migration.

## Goals / Non-Goals

**Goals:**

- Eliminate per-row `json_extract` in `all_symbols_summary`.
- Enable future WHERE/JOIN queries on `body_hash` (duplicate detection, identity diffing).
- Keep migration additive and safe under `synrepo upgrade`.

**Non-Goals:**

- No removal of `body_hash` from the JSON blob. Keeping it in both places makes the column a denormalised cache; rollback to the older binary continues to work because the JSON is still authoritative.
- No generated-column approach. The STORED-generated option was considered and rejected (see D1).
- No other JSON-embedded fields promoted in this change. Scope is `body_hash` only.
- No schema-version bump. The additive migration pattern does not require one.

## Decisions

### D1: Regular TEXT column with explicit insert-path updates, not a STORED generated column

SQLite supports `GENERATED ALWAYS AS (...) STORED` since 3.31.0 (2020), and indexing a stored generated column is supported. In principle that would let us avoid touching insert paths entirely. Rejected because:

- `WITHOUT ROWID` tables have had historical quirks with generated columns; forum reports of `ALTER TABLE ADD COLUMN … GENERATED` failing on `WITHOUT ROWID` tables, which is exactly our case.
- The additive-migration path already writes ALTER TABLE ADD COLUMN statements; adding one more and a handful of insert-site updates is low-risk and well-precedented.
- Insert-site updates are trivial (`symbol.body_hash.clone()` alongside the other bound params).

**Rationale**. Pick the approach that matches the existing codebase pattern. Performance is equivalent after backfill.

**Alternatives considered:**

- *VIRTUAL generated column with index*: VIRTUAL columns cannot be indexed by SQLite; discarded immediately.
- *STORED generated column (no insert-path changes)*: rejected per above — `WITHOUT ROWID` compatibility risk not worth the small code saving.

### D2: Backfill during migration, not lazily

Run `UPDATE symbols SET body_hash = json_extract(data, '$.body_hash') WHERE body_hash IS NULL` immediately after the ALTER TABLE. On a store with ~500k symbols this is a one-time cost paid at migration time; subsequent reads are clean.

**Rationale**. A lazy strategy (populate on next insert/upsert only) leaves existing rows with `NULL` body_hash forever, breaking the new contract of the dedicated column. Backfill is a single UPDATE inside the migration transaction.

### D3: Index the column

Create `idx_symbols_body_hash` on `symbols(body_hash)`. The current `all_symbols_summary` query does not use this index (it is a full scan), but future work (duplicate detection, identity diffing across revisions) will. Adding the index now is cheap and saves a separate migration later.

**Alternatives considered:**

- *Skip the index; add it when a WHERE-body_hash query actually ships*: rejected — indexes on TEXT columns are cheap, and a follow-on migration to add the index later would be another round of storage-compat work.

### D4: Keep `body_hash` in the JSON blob

Do not strip `body_hash` from the JSON blob. The `SymbolNode` struct in `src/structure/graph/node.rs:117` deserialises from JSON; removing the field would break reverse-compatibility. Keeping it in both places costs a few bytes per row.

**Rationale**. Denormalisation is fine when the write path populates both and the storage cost is trivial. Reverts to older binaries continue to work because the JSON is the source of truth.

### D5: Compatibility advisory

Add a `storage-and-compatibility` advisory so `synrepo upgrade` reports the migration:

> Advisory: The `symbols` table will gain a `body_hash` column and matching index on first access by this binary. The migration is additive and runs automatically; no data loss, backfill is inline.

Existing stores continue to work with the older binary (it ignores the new column); new rows written by the new binary populate the column.

## Risks / Trade-offs

- **Insert-path churn**: ~5 insert/upsert call sites in `src/store/sqlite/ops/` need to add one bound parameter. Mitigated by searching for existing bound-param patterns and mirroring them.

- **Backfill transaction size**: on very large stores (>1M symbols), the UPDATE inside the migration may hold the writer lock long enough to be noticed. Mitigation: batch the UPDATE in chunks of 50k rows via `WHERE body_hash IS NULL AND id > ? ORDER BY id LIMIT 50000`. Decide during implementation whether this is needed (measure on synrepo's own store first).

- **Rollback to older binary**: older binaries continue to work since `body_hash` is still in the JSON blob. No data loss.

- **Test-fixture drift**: several tests (`src/store/sqlite/tests/` and `src/store/overlay/findings_tests.rs`) construct symbols with literal `body_hash: "body".to_string()`. These tests exercise `SymbolNode` construction, not the SQL layer, so they are unaffected by the column addition.

## Migration Plan

1. Land the `migratables` entry + index creation first. Verify the new binary can read existing stores without populating the column (older rows return `NULL` on direct column read until backfill).
2. Land the backfill UPDATE inside the schema-init flow. Verify subsequent reads see populated rows.
3. Land the insert-path updates. New rows populate the column on write.
4. Land the query switch in `all_symbols_summary`. Drop the `json_extract` call.

No data migration required beyond the automatic backfill in step 2. Rollback is a matter of reverting the query switch; the column can remain in place harmlessly.

## Open Questions

- **O1**: Is the backfill UPDATE safe to run without batching on stores of size up to 1M symbols? Benchmark on a large clone (or synthesise one) before finalising batching policy.
- **O2**: Does the storage-compatibility check in `src/store/compatibility/evaluate/` need a new `SchemaHint` variant or does the existing additive-column pattern already cover it? Answer by reading `src/store/compatibility/evaluate/` during implementation.
