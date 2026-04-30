# synrepo canonical graph store (sqlite)

SQLite backend for `GraphStore`. Holds the canonical, parser-observed graph: files, symbols, concepts, edges, plus sidecar tables for drift scoring and compile-revision tracking.

On-disk location: `.synrepo/graph/nodes.db` (the `nodes.db` filename is internal to this module; callers pass the `.synrepo/graph/` directory to `SqliteGraphStore::open`).

## Where things live

| Concern | Location |
|---------|----------|
| Public type, open/open_existing, stats | `mod.rs` |
| PRAGMAs, `CREATE TABLE`, additive `ALTER` migrations | `schema.rs` |
| JSON row encode/decode, enum label encode | `codec.rs` |
| Compile-revision allocation, retirement, compaction | `lifecycle.rs` |
| `GraphStore` trait impl, method dispatch | `ops/mod.rs` |
| Node CRUD (file, symbol, concept) | `ops/nodes.rs` |
| Edge CRUD and traversal (outbound, inbound, all) | `ops/edges.rs` |
| `BEGIN`/`COMMIT` and re-entrant read snapshots | `ops/transactions.rs` |
| `all_file_paths`, `all_symbol_names`, summaries | `ops/lists.rs` |
| Drift scores and file fingerprints | `ops/drift.rs` |
| Cascade delete of nodes and their edges | `ops/helpers.rs` |
| Tests (deletion, persistence, snapshot, retirement) | `tests/` |

Overlay store is in a sibling module (`src/store/overlay/`) and a physically separate database. Overlay never mixes with graph.

## Schema

The full DDL (every column, every index, PRAGMAs, JSON blob layout) is in `docs/SCHEMA.md`. Schema changes go there and to `schema.rs` in the same commit.

This module owns the canonical sqlite implementation: `schema.rs` ships one `CREATE TABLE` per table and one `CREATE INDEX` per index, all in a single `execute_batch`. There are no migrations; schema bumps require `synrepo init` against a fresh `.synrepo/` (gated by `GRAPH_FORMAT_VERSION` in `src/store/compatibility/`).

## Observation lifecycle

Every mutation runs inside a compile revision allocated by `next_compile_revision_impl`. On re-observation:

1. Re-upserting a file/symbol/edge advances `last_observed_rev`.
2. A symbol or edge not re-emitted in the current pass is marked `retired_at_rev = <current_rev>` (soft delete).
3. A previously retired observation that re-appears has `retired_at_rev` cleared and `last_observed_rev` advanced.
4. `compact_retired(older_than_rev)` physically deletes retired rows whose retirement predates the cutoff and prunes `edge_drift` / `file_fingerprints` keyed by string revisions older than the cutoff. Driven by `retain_retired_revisions` (default 10) during `synrepo sync` and `synrepo upgrade --apply`.

`all_edges()` (and `active_edges()`, `outbound`, `inbound`, `symbols_for_file`, `edges_owned_by`) filter `retired_at_rev IS NULL`. Consumers that need retired rows (compaction enumeration) must query the `edges` table directly.

## Transactions and snapshots

Two lanes:

- **Writer** (`begin` / `commit` / `rollback`, `&mut self`): a standard `BEGIN DEFERRED`. `run_structural_compile` wraps stages 1-4 in one transaction; stage 4 reads uncommitted nodes via read-your-own-writes on the same connection.
- **Reader snapshot** (`begin_read_snapshot` / `end_read_snapshot`, `&self`): re-entrant depth counter guards a single `BEGIN DEFERRED`. Only the outermost begin opens the snapshot; only the outermost end commits. Nested snapshots inside an MCP handler and its `GraphCardCompiler` methods share the same committed epoch. `BEGIN DEFERRED` pins the epoch on the first `SELECT`, not at begin.

Writer and reader lanes must not interleave on the same handle. Cross-store writes go through the writer lock in `src/pipeline/writer/` (kernel advisory `flock` via `fs2`, not this module).

## Invariants enforced here

- `retired_at_rev IS NULL` filter on all list/traversal reads.
- JSON columns are the source of truth; typed columns mirror.
- `ConceptNode` upserts do not carry a machine-authored origin; the graph's `Epistemic` cannot hold a machine variant (invariant 1). Upserts accept whatever `ConceptNode` serialization the caller produced; the type boundary is enforced upstream in `src/structure/graph/`.
- ID encoding for edge endpoints uses `printf('sym_%016x', id)` to match `SymbolNodeId::to_string()` byte-for-byte. Do not change either side in isolation; the cascade delete in `ops/helpers.rs::delete_node_inner` relies on the match.

## Tests

Unit tests live under `tests/`:

- `persistence.rs` — upsert/get round-trips, JSON round-trip fidelity.
- `deletion.rs` — cascade deletes for files, symbols, concepts.
- `retirement.rs` — retire/unretire/compact end-to-end, drift/fingerprint pruning.
- `reader_snapshot.rs` — re-entrant depth counter behaviour.
- `support.rs` — shared fixtures.

Run a focused subset with `cargo test -p synrepo store::sqlite::tests::<name>`.
