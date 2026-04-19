## Why

Stage 8 of the structural pipeline is the atomic in-memory graph swap. It is the last unshipped stage per `AGENTS.md` ("ArcSwap commit — TODO stub") and is referenced in `src/lib.rs:15` and `src/pipeline/structural/mod.rs:16`.

The `arc-swap = "1"` dependency has been sitting in `Cargo.toml:88` (confirmed by grep: exactly one occurrence) since before the repo went public, with a comment that it is "for atomic snapshot publishing, matches syntext pattern". It is currently unused.

MCP tools and card handlers read the graph via SQLite on every query, wrapped in `with_graph_read_snapshot` (`BEGIN DEFERRED`). This works because:
- SQLite's `busy_timeout = 5000` absorbs transient WAL contention.
- The reader-snapshot guard prevents a reader from seeing two committed epochs across a single request.

But the per-request SQLite round-trip cost is load-bearing for the MCP hot path. `synrepo_module_card`, `synrepo_neighborhood`, and `synrepo_public_api` each kick off multiple `all_symbol_names`, `all_file_paths`, `outbound_edges` queries. Cards compile in tens of milliseconds when the graph is warm in page cache; they take hundreds of milliseconds on cold cache or large graphs. An in-memory snapshot behind `ArcSwap` collapses these queries to `HashMap`/`BTreeMap` lookups at the cost of memory proportional to graph size.

Stage 8 is the right place to build that snapshot because:
- It runs at the end of a successful structural compile — the point where the graph is consistent across stages 1–7.
- It can atomically swap a new snapshot into the shared `ArcSwap<Graph>` handle; readers see either the old or new version, never a partial commit.
- Writers continue to write to SQLite inside `with_writer_lock`; the in-memory snapshot is a second view, not the authority.

## What Changes

- Introduce `src/pipeline/structural/stage8.rs`:
  - A `pub fn run_graph_snapshot_commit(graph: &dyn GraphStore, snapshot_epoch: u64) -> crate::Result<()>` that:
    - Queries the full graph (files, symbols, concepts, edges) inside one `with_graph_read_snapshot` guard.
    - Builds an in-memory `Graph` struct with `HashMap<FileNodeId, FileNode>`, `HashMap<SymbolNodeId, SymbolNode>`, `HashMap<ConceptNodeId, ConceptNode>`, adjacency lists for each `EdgeKind`, and secondary indexes (`files_by_path`, `symbols_by_file`, `symbols_by_short_name`).
    - Stamps the snapshot with the `snapshot_epoch` (monotonic u64 tracked on the graph store).
    - Atomically swaps it into a process-global `ArcSwap<Graph>` handle.
  - Called at the tail of `run_structural_compile` after stage 7, inside the same writer transaction lock.
- Add `src/structure/graph/in_memory.rs`:
  - `pub struct Graph { ... }` — the in-memory snapshot type.
  - Methods mirroring `GraphStore` read methods: `all_symbol_names()`, `all_file_paths()`, `outbound_edges(...)`, `symbols_for_file(...)`, `file_node(...)`, `symbol_node(...)`, `concept_node(...)`.
  - An `Arc<Graph>` fetched via `snapshot()`; callers hold the `Arc` for the duration of their read, which guarantees consistency.
- Add `src/structure/graph/snapshot.rs`:
  - `pub static GRAPH_SNAPSHOT: ArcSwap<Graph>` — the process-global handle. Initialized to an empty `Graph` at module init; first reconcile replaces it.
  - `pub fn current() -> Arc<Graph>` — the read accessor.
  - `pub fn publish(new: Graph)` — the write accessor used only by stage 8.
- Switch MCP read-path callers to the snapshot:
  - `src/surface/mcp/cards.rs`, `src/surface/mcp/search.rs`, `src/surface/mcp/primitives.rs`, `src/surface/mcp/findings.rs` — replace `with_graph_read_snapshot(&graph, |graph| { ... })` with `let snapshot = graph::snapshot::current(); ...` where the request is purely a read.
  - Card compilation (`src/surface/card/compiler/*`) — extend `GraphCardCompiler` to accept either a `&dyn GraphStore` (fallback) or an `Arc<Graph>` (fast path). Implementation: new helper trait `GraphReader` that both types implement; card methods become generic over it.
- Writers remain unchanged: mutations go through `SqliteGraphStore`, take the writer lock, bump the `snapshot_epoch`, run stages 1–7, then run stage 8 which publishes the new snapshot.
- Add a status field: `synrepo status --full` reports `graph_snapshot_epoch`, `graph_snapshot_age_ms` (time since publish), and `graph_snapshot_nodes_total`.
- Memory bound: log a `tracing::warn!` when the snapshot size exceeds a configurable ceiling (default 500 MB), via a new `max_graph_snapshot_bytes` config field.

## Capabilities

### New Capabilities

- `graph-snapshot`: an atomically-published in-memory view of the graph, readable without a SQLite transaction. Independent of the SQLite store; used only by read-path MCP handlers.

### Modified Capabilities

- `structural-pipeline`: gains stage 8 (publish in-memory snapshot). Compile cycle now has all 8 stages wired.
- `mcp-surface`: hot-path read handlers query the in-memory snapshot first; SQLite is only used for mutations and for initial snapshot construction.
- `cards`: card compilers accept an `Arc<Graph>` fast path; SQLite fallback preserved for any caller that doesn't have a fresh snapshot.
- `storage-and-compatibility`: the SQLite store remains the authority. The snapshot is a derived view; if the process crashes mid-reconcile, the next reconcile rebuilds the snapshot from SQLite.

## Impact

- **Code**:
  - New: `src/structure/graph/in_memory.rs`, `src/structure/graph/snapshot.rs`, `src/pipeline/structural/stage8.rs`.
  - Modified: `src/pipeline/structural/mod.rs` — call `stage8::run_graph_snapshot_commit` at the end of `run_structural_compile`.
  - Modified: `src/surface/mcp/` (helpers, cards, search, primitives, findings) — hot-path reads switch to the snapshot.
  - Modified: `src/surface/card/compiler/` — card compiler gains a `GraphReader` trait; all existing methods rewritten to be generic over it.
  - Modified: `src/bin/cli_support/commands/status.rs` — new status fields.
  - Modified: `src/config.rs` — add `max_graph_snapshot_bytes` config field (default 500 MB).
  - Modified: `src/lib.rs:15` and `src/pipeline/structural/mod.rs:16` — docstrings no longer mention stage 8 as TODO.
- **APIs**: `GraphStore` trait stays unchanged (still the writer-side trait). A new `GraphReader` trait generalizes reads; cards switch to it internally. MCP tool signatures are unchanged.
- **Storage**: No SQLite schema changes. The snapshot lives only in memory; the disk store continues to be the authority.
- **Dependencies**: `arc-swap = "1"` is already present in `Cargo.toml:88`; no new deps.
- **Docs**:
  - `AGENTS.md` "Phase status" — mark stage 8 as shipped; update the sentence claiming it remains TODO.
  - `src/lib.rs` crate-level doc (line 15) — same update.
  - New "In-memory snapshot" subsection in `AGENTS.md` describing the read-path flow and memory bound.
- **Systems**: This change depends on `symbol-body-hash-column-v1` landing first (that change rehearses the additive SQLite-migration pattern; without it, the snapshot build query would hit the `json_extract` scan, slowing reconcile).

## User approval gate

Per the roadmap, this change requires user ask before implementation. Specific decisions needing sign-off:

1. **Scope of the snapshot**: proposal is full graph in memory. Alternative: file+symbol index only, edges stay in SQLite. Decide based on a measurement — how large is synrepo's graph today?
2. **Read-path blast radius**: every MCP handler touching the graph changes. Acceptance criterion for the change: zero behavior change observable from outside (same card outputs, same edge enumerations), just faster.
3. **Memory ceiling**: 500 MB default. Reasonable for current corpus sizes but a subjective call. Confirm or adjust.
