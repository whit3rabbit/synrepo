## 1. Schema Extension

- [x] 1.1 Add `first_seen_rev TEXT NULL` and `last_modified_rev TEXT NULL` columns to the `symbols` table schema in `src/store/sqlite/schema.rs`
- [x] 1.2 Add `first_seen_rev: Option<String>` and `last_modified_rev: Option<String>` fields to `SymbolNode` in `src/structure/graph/node.rs`
- [x] 1.3 Update `SqliteGraphStore` persistence layer (insert/upsert/read paths) to handle the two new columns
- [x] 1.4 Add compatibility check for the new columns in `src/store/compatibility/` and wire a migration action into `synrepo upgrade --apply`

## 2. Stage 5: Body-Hash Diffing

- [x] 2.1 Add a function in `src/pipeline/git/` that extracts the file content at a given sampled commit revision via `gix`
- [x] 2.2 Add a function that parses historical file content through the existing tree-sitter pipeline to produce a map of `(qualified_name, kind) -> body_hash`
- [x] 2.3 Implement the symbol-scoped revision derivation: walk sampled commits newest-to-oldest, diff `body_hash` values per qualified name, and produce `(first_seen_rev, last_modified_rev)` for each symbol present in the current compile
- [x] 2.4 Wire the derivation into stage 5 so that it runs after the file-level git mining, only for files that appear in the sampled commit list
- [x] 2.5 Write the derived revisions back to the graph store on the corresponding `SymbolNode` rows

## 3. Card Compiler Update

- [x] 3.1 Update `symbol_last_change_from_insights` in `src/surface/card/git.rs` to accept the symbol's stored revisions and prefer `last_modified_rev` when present
- [x] 3.2 When `last_modified_rev` is `Some`, resolve that revision's metadata (author, timestamp, summary) from the cached `GitPathHistoryInsights` and return `granularity: "symbol"`
- [x] 3.3 When `last_modified_rev` is `None`, fall back to the current file-level projection with `granularity: "file"` (no behavior change)

## 4. Tests

- [x] 4.1 Unit test: body-hash diffing produces correct `last_modified_rev` when a hash transition exists in the sampled window
- [x] 4.2 Unit test: body-hash diffing returns `None` for `last_modified_rev` when no transition exists
- [x] 4.3 Unit test: new symbol (qualified name absent from historical parses) gets `None` for both revisions
- [x] 4.4 Unit test: degraded history (no sampled commits for file) leaves both revisions `None`
- [x] 4.5 Card-level test: `SymbolCard.last_change` returns `granularity: "symbol"` when `last_modified_rev` is set
- [x] 4.6 Card-level test: `SymbolCard.last_change` returns `granularity: "file"` when `last_modified_rev` is `None`
- [x] 4.7 Snapshot test: update existing `SymbolCard` snapshots that reference `last_change.granularity`
- [x] 4.8 Run `make check` to verify fmt, clippy, and all tests pass

## 5. Spec Validation

- [x] 5.1 Run `openspec validate` to confirm the change artifacts are internally consistent
- [x] 5.2 Verify that `synrepo export` on the synrepo repo itself produces cards with `granularity: "symbol"` where applicable
