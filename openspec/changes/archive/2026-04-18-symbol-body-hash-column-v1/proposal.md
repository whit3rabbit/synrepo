## Why

`all_symbols_summary` at `src/store/sqlite/ops/lists.rs:71-102` reads five fields per row and uses `json_extract(data, '$.body_hash')` for one of them. The inline comment at line 73–74 explicitly frames this as an optimisation ("avoids a full deserialization while staying in one query"), which is true relative to parsing the full JSON blob with `serde_json::from_str`, but still forces SQLite to parse the JSON subtree on every row of the `symbols` table.

Phase 1 exploration confirmed:

- `body_hash` is stored only inside the `data` JSON blob in the `symbols` table.
- No SQL query filters on `body_hash` (a repo-wide grep shows only `json_extract(data, '$.body_hash')` as a SELECT, not a WHERE).
- The `symbols` table already uses an additive-migration pattern (`migratables` in `src/store/sqlite/schema.rs:71-133`) that ADD COLUMNs without a version bump; compat logic is established.
- `body_hash` is used throughout the codebase as a stable identity component for symbols (see `src/core/ids.rs:6-59`), so promoting it to a first-class column is consistent with its role.

The existing SELECT path runs in the `synrepo compact` analysis and in storage maintenance flows. On repositories with hundreds of thousands of symbols, the per-row `json_extract` adds measurable overhead relative to a direct column read. More importantly, a first-class column opens up future queries that filter or join on `body_hash` (duplicate detection, symbol-identity diffing across revisions) without regressing back to a full scan.

## What Changes

- Add a `body_hash` TEXT column to the `symbols` table via the existing `migratables` additive-migration pattern in `src/store/sqlite/schema.rs:71-133`.
- Backfill the new column during migration by running `UPDATE symbols SET body_hash = json_extract(data, '$.body_hash') WHERE body_hash IS NULL`.
- Add `CREATE INDEX IF NOT EXISTS idx_symbols_body_hash ON symbols(body_hash)` for future filter/join queries.
- Update every symbol-insert path (`src/store/sqlite/ops/` — symbol upsert and retirement paths) to populate `body_hash` alongside the JSON blob. Keep the JSON field as the authoritative record so existing deserialisation paths stay unchanged.
- Update `all_symbols_summary` at `src/store/sqlite/ops/lists.rs:71-102` to SELECT the new column directly (`SELECT id, file_id, qualified_name, kind, body_hash FROM symbols …`).
- Add a compatibility check in `src/store/compatibility/` that reports this migration as an advisory (additive, safe to run) and covers it with a round-trip test under `synrepo upgrade --apply`.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `storage-and-compatibility`: the symbols-table schema gains a `body_hash` column with an index. Additive migration, no version bump required (follows the existing pattern). Compatibility advisory notes the addition so operators see it in `synrepo upgrade`.

## Impact

- **Code**:
  - `src/store/sqlite/schema.rs` — add `body_hash` to `migratables`; add index creation.
  - `src/store/sqlite/ops/lists.rs` — switch query to read the column.
  - `src/store/sqlite/ops/` (insert/upsert paths) — write the column on insert.
  - `src/store/compatibility/` — advisory entry for the new column.
  - Tests in `src/store/sqlite/tests/` — confirm backfill and new-insert populate the column.
- **APIs**: No external API change. `SymbolSummary` return shape stays the same.
- **Dependencies**: None.
- **Systems**: Storage compatibility layer detects and backfills the column on first run of the updated binary.
- **Docs**: Brief update to the storage layout section of `AGENTS.md` / `CLAUDE.md` noting the new column.
