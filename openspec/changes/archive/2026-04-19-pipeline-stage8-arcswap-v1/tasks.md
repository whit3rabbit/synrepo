## 0. User approval gate

- [x] 0.1 Confirm with operator: scope is full graph in memory, not file+symbol index only. Record the answer in `notes.md`.
- [x] 0.2 Confirm with operator: 500 MB default memory ceiling. Adjust if the operator has tighter / looser constraints.
- [x] 0.3 Confirm with operator: this change lands after `symbol-body-hash-column-v1`. If that change is not merged, halt here.

## 1. Extract `GraphReader` trait

- [x] 1.1 In `src/structure/graph/mod.rs`, define `pub trait GraphReader: Send + Sync` with the read-only subset of current `GraphStore` methods (files, symbols, concepts, edges).
- [x] 1.2 Make `GraphStore: GraphReader` (extend, don't rewrite).
- [x] 1.3 Implement `GraphReader` for `SqliteGraphStore` by re-using the existing methods.
- [x] 1.4 Compile-check every caller of `GraphStore` read methods; update signatures to take `&dyn GraphReader` where they are read-only (this flags the places that truly need writer access).

## 2. Build in-memory `Graph`

- [x] 2.1 Create `src/structure/graph/in_memory.rs` with the `Graph` struct (fields per design.md D2) and `impl GraphReader for Graph`.
- [x] 2.2 Add `Graph::from_store(reader: &dyn GraphReader) -> crate::Result<Graph>` — builds every index from a read snapshot.
- [x] 2.3 Add `Graph::approx_bytes(&self) -> usize` — sum of heuristic node/edge sizes (not `size_of_val`, which misses heap). Used for the memory-ceiling warning.
- [x] 2.4 Unit tests: construct `Graph` from a small `SqliteGraphStore` fixture; call every `GraphReader` method and assert the result matches what the SQLite store returns for the same input.

## 3. Snapshot handle

- [x] 3.1 Create `src/structure/graph/snapshot.rs`:
  - `pub static GRAPH_SNAPSHOT: Lazy<ArcSwap<Graph>>` initialized with an empty `Graph`.
  - `pub fn current() -> Arc<Graph>`.
  - `pub fn publish(new: Graph)`.
- [x] 3.2 Export from `src/structure/graph/mod.rs`.
- [x] 3.3 Smoke test: publish two different `Graph` values; confirm `current()` reflects the latest store atomically.

## 4. Stage 8 implementation

- [x] 4.1 Create `src/pipeline/structural/stage8.rs` with `pub fn run_graph_snapshot_commit(graph: &dyn GraphStore, snapshot_epoch: u64) -> crate::Result<()>`.
- [x] 4.2 Inside the function: wrap reads in `with_graph_read_snapshot`; call `Graph::from_store`; stamp `snapshot_epoch`; size-check against `max_graph_snapshot_bytes`; publish via `snapshot::publish`.
- [x] 4.3 If `approx_bytes > max_graph_snapshot_bytes`, emit `tracing::warn!(...)` naming the file/symbol/edge counts and the actual size. Still publish.
- [x] 4.4 If `max_graph_snapshot_bytes == 0`, skip publishing entirely (operator opted out). Emit a one-time `tracing::info!` at process start.
- [x] 4.5 Wire the call at the end of `run_structural_compile` in `src/pipeline/structural/mod.rs`, after `COMMIT`. Thread `snapshot_epoch` from a u64 tracked on the graph store (increments every compile).

## 5. Add `max_graph_snapshot_bytes` config

- [x] 5.1 In `src/config.rs`, add `pub max_graph_snapshot_bytes: usize` with default `500 * 1024 * 1024`.
- [x] 5.2 Document the field in the `Config fields` table in `AGENTS.md`.

## 6. Card compiler integration

- [x] 6.1 In `src/surface/card/compiler/mod.rs`, introduce `enum GraphBackend { Sqlite(Box<dyn GraphStore>), Snapshot(Arc<Graph>) }`.
- [x] 6.2 `GraphCardCompiler::new(Box<dyn GraphStore>, Option<&Path>)` stays working (SQLite backend).
- [x] 6.3 Add `GraphCardCompiler::new_with_snapshot(Arc<Graph>, Option<&Path>)`.
- [x] 6.4 Every card-compiler method that takes `&dyn GraphStore` or `self.graph` now dispatches on the backend. Prefer a `GraphReader` view so the dispatch lives in one place.
- [x] 6.5 Run the existing card test suite; no behavior change expected.

## 7. MCP read-path switch

- [x] 7.1 In `src/surface/mcp/cards.rs`, identify every handler that is purely a read. Replace `with_graph_read_snapshot` blocks with `let snapshot = graph::snapshot::current();`; pass it into the card compiler via `new_with_snapshot`.
- [x] 7.2 Do the same for `src/surface/mcp/search.rs`, `src/surface/mcp/primitives.rs`, `src/surface/mcp/findings.rs`.
- [x] 7.3 Leave mutating tools (e.g., anything under `src/bin/cli_support/commands/links.rs`, `setup.rs`, `sync.rs`) on the SQLite path.
- [x] 7.4 Parity test: for a scratch repo, run each migrated tool once with the snapshot active and once with `SYNREPO_DISABLE_GRAPH_SNAPSHOT=1` (new debug env var that forces SQLite). Assert identical outputs.

## 8. Status surface

- [x] 8.1 In `src/bin/cli_support/commands/status.rs`, add three fields to the status payload: `snapshot_epoch: u64`, `snapshot_age_ms: u64`, `snapshot_size_bytes: usize`.
- [x] 8.2 Populate from `graph::snapshot::current()`; size via `Graph::approx_bytes`; age via a `published_at: SystemTime` on `Graph`.
- [x] 8.3 Render in `status --full` text output and in the JSON mode.

## 9. Docs

- [x] 9.1 Update `AGENTS.md` — "Structural pipeline stage status" section: stage 8 moves from TODO to shipped.
- [x] 9.2 Update `src/lib.rs:15` — remove the "Stage 8 (ArcSwap commit) is still a TODO" line.
- [x] 9.3 Update `src/pipeline/structural/mod.rs:16` — rewrite the stage-status paragraph to reflect all 8 stages live.
- [x] 9.4 Add a new "In-memory snapshot" subsection in `AGENTS.md` under "Architecture" — describe `GraphReader`, `ArcSwap<Graph>`, when the snapshot publishes, and the memory ceiling.
- [x] 9.5 Update `openspec/specs/foundation/spec.md` stage-8 row (if it has one) to remove the "TODO" marker.

## 10. Tests

- [x] 10.1 Unit tests for `Graph::from_store` (task 2.4).
- [x] 10.2 `src/pipeline/structural/tests/stage8.rs` — integration test: bootstrap a fixture, run compile, assert `snapshot::current().snapshot_epoch > 0` and file count matches.
- [x] 10.3 MCP parity test per task 7.4.
- [x] 10.4 Memory ceiling test: set `max_graph_snapshot_bytes = 1`; run compile; assert the warning fires and the snapshot still publishes.
- [x] 10.5 Full `make check` + `cargo test --test mutation_soak -- --ignored --test-threads=1`.

## 11. Benchmark

- [x] 11.1 Before-change: time `synrepo_module_card` on a medium directory of synrepo itself; record wall-clock time. Run 10 iterations, take median.
- [x] 11.2 After-change: same measurement with the snapshot active.
- [x] 11.3 Expected result: 5–50x faster for purely-read tools. Record the ratio in the task log.
- [x] 11.4 Measure snapshot build time at the end of `run_structural_compile` on synrepo. Expected: low single-digit seconds.

## 12. Archive

- [x] 12.1 Run `openspec validate pipeline-stage8-arcswap-v1 --strict`.
- [x] 12.2 Invoke `opsx:archive` with change id `pipeline-stage8-arcswap-v1`.
