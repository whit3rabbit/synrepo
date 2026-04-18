## Context

The graph today has one storage layer: `SqliteGraphStore`. Every read — cards, MCP handlers, drift surfaces, export — goes through `with_graph_read_snapshot`, which begins a deferred transaction, runs N queries, and ends the transaction. SQLite's WAL, `busy_timeout = 5000`, and reader-snapshot reentrancy make this safe; it is not fast.

The arc-swap pattern is the standard hot-read solution: build a fully-indexed immutable graph in memory at compile time, wrap it in `ArcSwap<Graph>`, and let readers clone an `Arc` for consistent reads without contention. The syntext lexical index already uses this pattern — our `Cargo.toml` comment at line 88 refers to it.

The challenge is not `ArcSwap` itself (30 lines); it is making readers consume the snapshot without breaking the SQLite fallback. Cards compile via `GraphCardCompiler` which holds `Box<dyn GraphStore>`. MCP handlers wrap reads in `with_graph_read_snapshot`. Everything that reads today does so via the `GraphStore` trait. We need a read-only trait that both `SqliteGraphStore` and the new in-memory `Graph` can implement, and cards need to be generic over that trait.

## Goals / Non-Goals

**Goals:**

- MCP hot-path reads take O(1) map lookups rather than O(queries-per-card × SQLite round-trips).
- Snapshot is atomically replaced at the end of every structural compile. Readers never see a partial state.
- SQLite remains the authority. A process restart rebuilds the snapshot from SQLite on first reconcile.
- No observable behavior change: card output, edge enumeration, neighborhood expansion all produce identical results to the pre-change SQLite path.

**Non-Goals:**

- No persistence for the snapshot. It is rebuilt on every compile; faster than loading a serialized blob.
- No incremental snapshot updates. Stage 8 rebuilds the whole thing. If reconcile latency becomes a problem, revisit.
- No multi-process snapshot sharing. The snapshot is per-process. MCP servers running in separate processes each hold their own `ArcSwap` and rebuild via the same compile path.
- No change to writer semantics. Mutations still take `with_writer_lock`, still write SQLite, still emit JSONL repair logs.
- No change to the `overlay` store. Overlay reads are a separate path; this change does not touch them. (If the overlay-read pattern proves similarly load-bearing, a follow-up change can add an overlay snapshot.)

## Decisions

### D1: `GraphReader` trait, not `GraphStore::Read`

Add a new trait in `src/structure/graph/mod.rs`:

```rust
pub trait GraphReader: Send + Sync {
    fn all_file_paths(&self) -> crate::Result<Vec<(String, FileNodeId)>>;
    fn all_symbol_names(&self) -> crate::Result<Vec<(SymbolNodeId, FileNodeId, String)>>;
    fn file_node(&self, id: FileNodeId) -> crate::Result<Option<FileNode>>;
    fn symbol_node(&self, id: SymbolNodeId) -> crate::Result<Option<SymbolNode>>;
    fn concept_node(&self, id: ConceptNodeId) -> crate::Result<Option<ConceptNode>>;
    fn symbols_for_file(&self, id: FileNodeId) -> crate::Result<Vec<SymbolNode>>;
    fn outbound_edges(&self, from: NodeId, kind: Option<EdgeKind>) -> crate::Result<Vec<Edge>>;
    fn inbound_edges(&self, to: NodeId, kind: Option<EdgeKind>) -> crate::Result<Vec<Edge>>;
    fn all_edges(&self, kind: Option<EdgeKind>) -> crate::Result<Vec<Edge>>;
    fn count_edges_by_kind(&self, kind: EdgeKind) -> crate::Result<usize>;
    // ... etc., whatever read methods `GraphCardCompiler` uses today.
}
```

Both `SqliteGraphStore` and `in_memory::Graph` implement it. `GraphStore` (writer trait) extends `GraphReader`.

Rationale: separating reader from writer traits is the standard split; it makes the snapshot implementation obvious (reads only) and keeps the writer surface intact. The alternative — a `GraphStore::Read` associated type — would require wider changes and obscure the intent.

### D2: Snapshot is a `Graph` struct built at stage 8

```rust
pub struct Graph {
    pub snapshot_epoch: u64,
    pub files: HashMap<FileNodeId, FileNode>,
    pub files_by_path: HashMap<String, FileNodeId>,
    pub symbols: HashMap<SymbolNodeId, SymbolNode>,
    pub symbols_by_file: HashMap<FileNodeId, Vec<SymbolNodeId>>,
    pub symbols_by_short_name: HashMap<String, Vec<SymbolNodeId>>,
    pub concepts: HashMap<ConceptNodeId, ConceptNode>,
    pub concepts_by_path: HashMap<String, ConceptNodeId>,
    pub edges_by_from: HashMap<NodeId, Vec<Edge>>,
    pub edges_by_to: HashMap<NodeId, Vec<Edge>>,
    pub edges_by_kind: HashMap<EdgeKind, Vec<Edge>>,
}
```

All indexes pre-computed at build time. Readers pay only the cost of the `HashMap` lookup and a clone (either of the node or of a small `Vec<Edge>`).

Cost estimate for synrepo itself: ~1,000 files × 200 bytes + ~10,000 symbols × 500 bytes + ~30,000 edges × 150 bytes = ~10 MB. Well under the 500 MB default ceiling. For a medium corpus (e.g., the Linux kernel ~70,000 files), expect closer to 200–300 MB.

### D3: Stage 8 runs inside the same transaction as stages 1–7, but after commit publishes

The compile lifecycle:

1. Writer lock acquired.
2. `BEGIN` SQLite transaction.
3. Stages 1–7 run (discover, parse, prose, cross-file edges, git mining, identity, drift).
4. `COMMIT` SQLite transaction.
5. **Stage 8: rebuild `Graph` from the committed SQLite state; publish via `ArcSwap::store`.**
6. Writer lock released.

Stage 8 queries SQLite inside a `with_graph_read_snapshot` guard, which is a read-path and safe to run after the writer's `COMMIT`. It does not re-enter the writer lock; it does not hold it beyond normal SQLite contention.

**Why after commit, not before**: if the commit fails (rare, but possible — WAL disk full, etc.), we do not want to publish a snapshot based on uncommitted writes.

### D4: Integration with existing `GraphCardCompiler`

`GraphCardCompiler::new(Box<dyn GraphStore>)` stays supported — that is the SQLite fallback path, used by tests, by the initial bootstrap before the first snapshot publishes, and by any consumer that wants the authoritative SQLite view.

Add `GraphCardCompiler::new_with_snapshot(Arc<Graph>)`. Internally, the compiler holds an enum:

```rust
enum GraphBackend {
    Sqlite(Box<dyn GraphStore>),
    Snapshot(Arc<Graph>),
}
```

and its methods call through to the backing reader. All existing card-compiler methods become generic over `GraphBackend`; a single dispatch layer drives both.

Alternative considered: make `GraphCardCompiler` always take `Arc<dyn GraphReader>`. Rejected because it forces a dynamic-dispatch vtable even for the hot snapshot path, which defeats part of the win.

### D5: MCP read-path switch

MCP handlers today look like:

```rust
fn handle_foo(state: &SynrepoState, args: &Args) -> Result<Output> {
    with_graph_read_snapshot(&state.graph, |graph| {
        // ... queries against graph ...
    })
}
```

After the change:

```rust
fn handle_foo(state: &SynrepoState, args: &Args) -> Result<Output> {
    let snapshot = graph::snapshot::current();
    // ... queries against snapshot ...
}
```

`SynrepoState::graph` is still useful for mutating tools (`synrepo_set_declared_link`, etc.). Read-only tools shift to the snapshot.

A small risk: if the MCP server starts before the first compile, the snapshot is empty. Mitigation: `graph::snapshot::current()` returns an empty `Arc<Graph>` at startup; readers see zero files/symbols/edges. This is the correct behavior — the bootstrap flow runs `synrepo init`, which runs the structural compile, which publishes the snapshot before any MCP client can connect. The `synrepo mcp` subcommand at `src/bin/cli_support/commands/mcp.rs` runs `bootstrap()` first, so by the time the server accepts connections the snapshot is live.

### D6: Memory ceiling and warning

New config field `max_graph_snapshot_bytes: usize` with default 500 MB. Stage 8 measures the snapshot size via a deterministic accountant (e.g., `std::mem::size_of_val` per node plus HashMap overhead) before publishing. If the snapshot exceeds the ceiling:

- Emit `tracing::warn!` naming the file count, symbol count, and actual size.
- Still publish. The ceiling is advisory, not enforced. Rationale: a production deployment on a large corpus should see the warning and either raise the ceiling or move to a degraded SQLite-only mode. Silently refusing to publish would put the MCP surface in an inconsistent state (stale snapshot while SQLite moves forward).

Future: if `max_graph_snapshot_bytes` is exceeded, a future change could skip building certain indexes (e.g., `edges_by_kind` for rare kinds) to stay under budget. Out of scope for v1.

### D7: Status surface

`synrepo status --full` gains three lines:

```
graph snapshot epoch: 42
graph snapshot age: 3.4s
graph snapshot size: 12.7 MB (1200 files / 9500 symbols / 28000 edges)
```

Computed at status-time from `graph::snapshot::current()`.

JSON output: nested object `"graph_snapshot": { "epoch": 42, "age_ms": 3400, "size_bytes": 13312000, "files": 1200, ... }`.

### D8: Tests

Three layers:

- **Unit tests in `in_memory.rs`** — constructing a `Graph` from a small fixture and verifying every `GraphReader` method returns the same result as the SQLite store for the same data.
- **Integration test in `src/pipeline/structural/tests/stage8.rs`** — run a full compile, verify the snapshot epoch bumped, verify a read against the snapshot returns the expected file count.
- **MCP parity test** — compile a small fixture, then run each read-only MCP tool twice: once with the snapshot active and once with snapshot disabled (`SYNREPO_DISABLE_GRAPH_SNAPSHOT=1` as a debug-only env bypass). Assert identical outputs.

No soak/load tests in this change. Benchmark numbers belong in the task log, not in the test suite.

## Risks / Trade-offs

- **Double memory cost**: the snapshot lives alongside the SQLite page cache. On a server with tight memory, this could push other processes out. Mitigation: the ceiling warning; operators can set `max_graph_snapshot_bytes = 0` to disable the snapshot (falls back to pure SQLite — not in v1 scope, but the enum structure permits it).

- **Snapshot staleness between compiles**: readers see the most recent compile's state, even if a new compile is in progress. This is correct (ArcSwap guarantees atomicity) but means a reader can read stale data if a compile started 5 seconds ago and has not yet published. Existing behavior with `with_graph_read_snapshot` is similar: the snapshot pins to the writer state at begin-time; any commit during the read is invisible.

- **Snapshot rebuild cost on every reconcile**: for a 200 MB snapshot, rebuilding in memory is seconds, not milliseconds. Reconcile takes longer by that amount. Mitigation: stage 8's rebuild runs in a single `with_graph_read_snapshot` pass — no N+1 queries — so it should be dominated by SQLite scan time, which is already paid.

- **Trait explosion**: `GraphReader` adds another trait to maintain alongside `GraphStore`. Mitigation: `GraphReader` is extracted from existing read methods on `GraphStore`; it is a subset, not a parallel surface. The refactor is mechanical.

- **`synrepo mcp` startup ordering**: if MCP handlers read the snapshot before `bootstrap()` has published one, they see empty results. Mitigation: the startup path already calls `bootstrap()` before accepting connections. Add an assertion: the first read via `snapshot::current()` after startup must return a snapshot with `snapshot_epoch > 0`, or log a loud warning.

- **Rollback cost**: once readers switch to the snapshot, reverting the change means putting SQLite reads back on the hot path. Latency regression is immediate. Mitigation: keep the SQLite fallback path in `GraphCardCompiler` operational; a feature flag (`SYNREPO_DISABLE_GRAPH_SNAPSHOT=1`) can route reads back through SQLite for debugging or rollback without a revert.

## Migration Plan

Single PR. Sequencing inside the PR:

1. Add `GraphReader` trait in `src/structure/graph/mod.rs`. Implement it for `SqliteGraphStore` by extracting existing read methods. Verify all existing callers still compile.
2. Add `Graph` struct in `src/structure/graph/in_memory.rs` with `GraphReader` impl. Write the unit tests.
3. Add `src/structure/graph/snapshot.rs` with the `ArcSwap<Graph>` handle.
4. Add `src/pipeline/structural/stage8.rs`; wire it at the end of `run_structural_compile`.
5. Refactor `GraphCardCompiler` to accept either backend.
6. Switch MCP read handlers to the snapshot one tool at a time; run the parity test after each.
7. Update docs (AGENTS.md, lib.rs, structural/mod.rs).
8. Remove the "stage 8 TODO" mentions in `src/lib.rs:15` and `src/pipeline/structural/mod.rs:16`.

Rollback: revert the PR. No data migration; SQLite is still the authority, so the graph persists untouched.

This change depends on `symbol-body-hash-column-v1` landing first. That change is a lower-risk rehearsal of the additive-SQLite-migration pattern and reduces the cost of the stage-8 snapshot-build scan (which iterates all symbols).

## Open Questions

- **O1**: Should `max_graph_snapshot_bytes = 0` disable the snapshot entirely, or should it hard-fail? Current proposal: disable (fall back to SQLite). Confirms operator intent.
- **O2**: Should the snapshot include retired nodes (for compaction metadata) or only active? Current proposal: active only — matches `active_edges()`, matches what readers want.
- **O3**: Is `ArcSwap` the right primitive, or would `Arc<RwLock<Arc<Graph>>>` suffice? Current proposal: `ArcSwap` — lock-free reads are the whole point; RwLock reintroduces contention we are trying to eliminate.
- **O4**: Should stage 8 run unconditionally or only when a flag is set? Current proposal: unconditional, with the bypass env var for rollback. Having the snapshot active on every reconcile is what makes the MCP path fast by default.
