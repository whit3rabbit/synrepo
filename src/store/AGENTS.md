# AGENTS.md

> `CLAUDE.md` is a symlink to `AGENTS.md`. Edit `AGENTS.md`; both update.

Storage layer: SQLite backends for graph and overlay.

## Key files

- `mod.rs` — module declarations only; trait definitions live in `src/structure/graph/` (`GraphStore`) and `src/overlay/mod.rs` (`OverlayStore`)
- `sqlite/mod.rs` — `SqliteGraphStore` implementation
- `sqlite/schema.rs` — graph schema, `busy_timeout = 5000`
- `sqlite/ops/` — CRUD operations split across `drift.rs`, `edges.rs`, `helpers.rs`, `lists.rs`, `mod.rs`, `nodes.rs`, `transactions.rs`
- `sqlite/codec.rs` — row (de)serialization helpers
- `sqlite/lifecycle.rs` — retirement, compaction
- `overlay/mod.rs` — overlay store, cross-link audit
- `overlay/schema.rs` — overlay schema
- `compatibility/` — version checks, migration policy

## Hard invariants

- Graph store: `.synrepo/graph/nodes.db`
- Overlay store: `.synrepo/overlay/` (separate from graph)
- Reader snapshots are re-entrant (depth counter, BEGIN DEFERRED only on outermost)
- Writer lock uses `fs2` (kernel advisory lock), not file existence
- Retired observations soft-deleted until compaction (`retain_retired_revisions` config, default 10)
- `all_edges()` excludes retired edges
