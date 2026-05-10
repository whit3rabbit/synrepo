# Architecture

synrepo is a local deterministic code-context compiler:

```text
repo files -> graph facts -> code artifacts -> task contexts -> cards/MCP
```

The codebase is organized as five product layers plus orchestration and runtime
modules. The layer boundary is a trust and ownership boundary, not a strict Rust
import DAG. Current code already has substrate discovery using pipeline Git
helpers and optional embedding code reading `GraphStore` for chunk extraction,
so a blanket "no layer may import above it" rule is false.

Preserve these invariants instead:

- The canonical graph contains directly observed parser, Git, or human-declared
  facts. `graph::Epistemic` stays limited to `ParserObserved`, `HumanDeclared`,
  and `GitObserved`.
- Machine-authored content uses overlay types and stays in
  `.synrepo/overlay/overlay.db`. It must not be promoted into graph fact types
  without a human-declared path.
- Explain retrieval filters to `source_store = "graph"` and never reads overlay
  output as explain input.
- Multi-query reads through graph or overlay stores must use read snapshots so
  one operation does not observe two committed epochs.
- New or changed Rust files should stay under 400 lines. At validation time,
  a small number of existing test files were over the target; do not grow them.

## Layers

**0. Core** (`src/core/`) - Shared low-level types and helpers.

- `ids.rs` - stable identifier types: `FileNodeId`, `SymbolNodeId`,
  `ConceptNodeId`, `EdgeId`, and unified `NodeId`. IDs are backed by 128-bit
  blake3-derived values and serialize as prefixed 32-character hex strings
  such as `file_...`.
- `provenance.rs` - `Provenance`, `CreatedBy`, and `SourceRef` for graph rows
  and source-derived observations. Overlay entries use their own provenance,
  lifecycle, and audit fields where needed.
- `source_language.rs` - centralized supported-language extension registry used
  by discovery and parser dispatch.
- `path_safety.rs` and `project_layout/` - path validation and advisory project
  layout detection.

**1. Substrate** (`src/substrate/`) - File discovery, classification, indexing,
and local search inputs.

- `discover.rs` - filesystem walk via `ignore`; respects `.gitignore`,
  `.synignore`, redaction globs, configured roots, linked worktrees by default,
  and initialized submodules when enabled.
- `classify.rs` - maps files to `FileClass` values such as supported code,
  text code, markdown, Jupyter, or skipped.
- `index.rs` and `rooted_search.rs` - `syntext` lexical index build/query
  paths for `.synrepo/index/`.
- `incremental.rs` - incremental lexical index maintenance for trustworthy
  touched-path batches.
- `hybrid.rs` - reciprocal-rank fusion of lexical and vector results when
  semantic triage is locally available.
- `embedding/` - optional semantic-triage stack with local ONNX and local
  Ollama providers. Query-time loading never downloads model assets.

**2. Structure** (`src/structure/`) - The canonical graph of directly observed
facts.

- `graph/` - graph node and edge types, `EdgeKind`, `SymbolKind`, `Epistemic`,
  `GraphStore`, `GraphReader`, and in-memory graph snapshots.
- `parse/` - tree-sitter dispatch for Rust, Python, TypeScript, TSX, Go,
  JavaScript, Java, Kotlin, C#, PHP, Ruby, Swift, C, C++, and Dart. It extracts
  symbols plus call/import references used by later stages.
- `prose.rs` - markdown concept extraction from configured human-authored
  concept directories.
- `identity/` - root-scoped file identity and rename detection.
- `drift.rs` - per-edge drift scoring from persisted structural fingerprints.
- `rationale.rs` - inline `DECISION:` marker extraction for supported code
  comments (`//` for Rust/TypeScript/TSX, `#` for Python). These markers are
  stored on `FileNode.inline_decisions`; they cannot produce `ConceptNode`.

Node types: `FileNode` has root-discriminated content-hash identity,
`SymbolNode` is parser-extracted, and `ConceptNode` is created only from
human-authored markdown in configured directories such as `docs/concepts/`,
`docs/adr/`, and `docs/decisions/`. Explain cannot create concept nodes.

**3. Overlay** (`src/overlay/`) - Machine-authored and advisory content in a
physically separate SQLite database from the graph.

- `OverlayStore` persists commentary entries, proposed cross-links, agent notes,
  note transitions, note links, lifecycle state, audit rows, and compaction
  surfaces.
- `OverlayLink`, `OverlayEpistemic`, and `CitedSpan` represent machine-authored
  cross-link candidates. Accepted curated links become graph edges with
  `Epistemic::HumanDeclared`; candidates remain in overlay audit history.
- Read consistency uses `with_overlay_read_snapshot`, mirroring graph snapshots
  for multi-query overlay reads.

**4. Surface** (`src/surface/`, `src/bin/cli.rs`, `src/bin/cli_support/`) -
CLI, cards, MCP, setup shims, and serialized delivery packets.

- `card/` - stable card surface and `GraphCardCompiler`, including file,
  symbol, decision, module, call-path, public-API, entry-point, test-surface,
  change-risk, neighborhood, Git, and overlay-link projections.
- `mcp/` - MCP tool handlers for ask, cards, context packs, search, impact,
  overview, graph primitives, commentary, docs, notes, edits, compact,
  readiness, resume context, refactor suggestions, and task routing.
- `changed`, `context`, `graph_view`, `handoffs`, `readiness`,
  `resume_context`, `status_snapshot`, and `task_route` are read-model surfaces
  over graph, index, overlay, and runtime state.
- `src/bin/cli.rs` is the binary entrypoint and dispatcher. Command logic lives
  mostly under `src/bin/cli_support/`.

## Bootstrap

`src/bootstrap/` handles first-run UX, mode detection, runtime initialization,
repair-friendly refreshes, and bootstrap reports.

- `init/` - bootstrap orchestrators. `bootstrap(repo_root, requested_mode,
  update_gitignore)` delegates to force/config variants, evaluates runtime
  compatibility, acquires the writer lock, writes config, writes
  `.synrepo/.gitignore`, builds the lexical index, opens graph and overlay
  stores, runs structural compile, emits best-effort co-change edges, finishes
  runtime surfaces, persists reconcile state, writes compatibility state, and
  records the install registry entry.
- `mode_inspect.rs` - auto vs curated mode detection.
- `report.rs` - `BootstrapReport`, `BootstrapHealth`, and `BootstrapAction`.

Bootstrap derives `.synrepo/` from `Config::synrepo_dir(repo_root)`, but guided
setup can seed config through `bootstrap_with_force_and_config`.

## Pipeline

`src/pipeline/` owns reconcile-time production, repair, watch, explain,
maintenance, Git intelligence, context metrics, compaction, and writer locking.

- `structural/` - logical compile stages. `mod.rs` owns transaction
  orchestration and `CompileSummary`; `stages/` owns discovery/code/prose
  processing plus identity cascade; `stage4/` owns cross-file call/import
  resolution; `stage8.rs` publishes graph snapshots.
- `git/` and `git_intelligence/` - first-parent history sampling, rename
  helpers, history/hotspot/ownership/co-change summaries, co-change edge
  emission, and per-symbol revision tracking.
- `repair/` - drift reporting and sync actions. Current `RepairSurface` has
  12 surfaces: store maintenance, structural refresh, writer lock, declared
  links, stale rationale, commentary overlay entries, proposed links overlay,
  agent notes overlay, export surface, edge drift, retired observations, and
  legacy agent installs. Sync handlers live under `repair/sync/`.
- `watch/` - watch lease/control plane, event filtering, pending-event
  coalescing, reconcile backstop, auto-sync, embedding jobs, daemon service,
  and status reporting.
- `writer/` - single-writer admission, metadata, kernel advisory lock sentinel,
  re-entrancy, retry helpers, and cross-platform contention checks.
- `explain/` - `CommentaryGenerator` trait, no-op default, provider backends,
  commentary refresh, docs export/import/search, and telemetry.
- `maintenance.rs`, `compact/`, and `context_metrics/` - compatibility cleanup,
  runtime compaction, and context-serving metrics.

### Structural stages

Structural parsing is wired for Rust, Python, TypeScript, TSX, Go, JavaScript,
Java, Kotlin, C#, PHP, Ruby, Swift, C, C++, and Dart.

1. **Discover** - substrate walk, `.gitignore`/`.synignore` respected per
   discovery root. The root set is the primary checkout plus linked worktrees by
   default; initialized submodules are opt-in and walked as separate roots.
2. **Parse code** - tree-sitter symbol extraction; emits `FileNode`,
   `SymbolNode`, and `Defines` edges, plus pending call/import references.
3. **Parse prose** - concept node extraction from configured markdown
   directories.
4. **Cross-file edge resolution** - emits `Calls` and `Imports`. Call scoring
   currently uses +100 same-file, +50 imported, +20 public, +10 crate, -100
   private cross-file, +30 kind, and +40 prefix. Edges emit only when score is
   positive and the winner is unique or tied at least 50. Import resolvers cover
   TypeScript/JavaScript relative imports, Python dotted modules, Rust
   `crate::`/`self::`/`super::` and bare crate paths, Go module-prefix imports,
   Dart package/relative imports, and Java/Kotlin source-root imports.
   `Inherits`, `References`, and `Mentions` are not yet emitted.
5. **Git mining** - deterministic first-parent history sampling per discovery
   root; emits `CoChangesWith` edges through `git_intelligence/emit.rs`.
   Cross-root Git edges are not emitted.
6. **Identity cascade** - content-hash rename, edited single-file rename,
   split/merge detection, Git rename fallback, and breakage detection. It emits
   `SplitFrom` and `MergedFrom`, preserves `FileNodeId` across same-root
   renames, and records old paths in `path_history`.
7. **Drift scoring** - Jaccard distance on persisted structural fingerprints;
   writes sidecar `edge_drift` and `file_fingerprints` tables.
8. **Per-repo snapshot publish** - rebuilds immutable `Graph` from SQLite and
   publishes it through `structure::graph::snapshot` when it fits the configured
   memory budget.

Stages 1-4 run in a single atomic SQLite transaction in
`run_structural_compile`. Stage 4 reads uncommitted nodes from stages 1-3 via
read-your-own-writes on the same connection. Watch-triggered reconciles can
scope the structural write to the discovery root that owns the changed path,
leaving sibling roots untouched.

#### Body-Scope Calls

Call extraction carries the enclosing caller symbol's `(qualified_name,
body_hash)` pair. Stage 4 resolves that pair against active symbols from the
same transaction and emits a symbol-to-symbol `Calls` edge when the callee scores
to a positive candidate. If no enclosing caller exists, for example a
module-top-level statement, stage 4 keeps only the file-to-symbol `Calls` edge.

### Overlay and audit surfaces

- Cross-link overlay store, card surfacing, CLI review flow, MCP findings/audit
  tools, and `synrepo sync --generate-cross-links` /
  `--regenerate-cross-links` are implemented.
- Cross-link promotion remains curated-mode-only. Accepted links become graph
  edges with `Epistemic::HumanDeclared`; overlay candidates stay in audit
  history.
- Commentary and agent-note repairs are overlay operations and must not mutate
  canonical graph facts.

## Store

`src/store/` contains SQLite backends and runtime compatibility checks.

- `sqlite/` - `SqliteGraphStore`, opening `.synrepo/graph/nodes.db` from the
  graph directory and implementing `GraphStore`.
- `overlay/` - `SqliteOverlayStore`, opening `.synrepo/overlay/overlay.db` and
  implementing `OverlayStore`.
- `compatibility/` - runtime layout checks, store versioning, compatibility
  snapshots, rebuild/clear/block policy, and maintenance inputs.

Code that opens the graph store uses `SqliteGraphStore::open(&graph_dir)` where
`graph_dir` is `.synrepo/graph/`; the `nodes.db` filename is internal to the
SQLite backend.

## Storage layout

- `.synrepo/graph/nodes.db` - canonical SQLite graph store.
- `.synrepo/overlay/overlay.db` - overlay SQLite store, physically separate
  from graph tables.
- `.synrepo/index/` - `syntext` lexical index.
- `.synrepo/index/vectors/` - optional flat-vector semantic index.
- `.synrepo/embeddings/` - local embedding runtime artifacts when present.
- `.synrepo/cache/llm-responses/` - disposable LLM response cache.
- `.synrepo/config.toml` - runtime config loaded by `Config`.
- `.synrepo/.gitignore` - ignores everything under `.synrepo/` except
  `.gitignore`; `config.toml` is intentionally not unignored.
- `.synrepo/explain-docs/` - advisory commentary docs generated from overlay.
- `.synrepo/explain-index/` - `syntext` index over editable explain docs.
- `.synrepo/state/writer.lock` - writer ownership metadata while a writer holds
  the lock.
- `.synrepo/state/writer.lock.flock` - kernel advisory lock sentinel.
- `.synrepo/state/watch-daemon.json` - per-repo watch lease plus owner and
  telemetry snapshot. It records the active control endpoint.
- `.synrepo/state/watch-daemon.json.flock` - watch lease sentinel flock file.
- `.synrepo/state/watch-daemon.log` - stderr of the detached watch daemon.
- `.synrepo/state/reconcile-state.json` - last reconcile outcome, timestamp,
  triggering event count, and discovered/symbol counts.
- `.synrepo/state/repair-log.jsonl` - append-only resolution log for
  `synrepo sync`.
- `.synrepo/state/storage-compat.json` - runtime compatibility snapshot.
- `.synrepo/state/context-metrics.json` - context-serving metrics.
- `.synrepo/state/compact-state.json` - last compaction timestamp, when
  compaction has run.
- `.synrepo/state/explain-log.jsonl`, `explain-totals.json`, and
  `explain-scope.json` - explain telemetry, aggregates, and persisted UI scope.
- Watch control sockets are not stored under `.synrepo/state/`. On Unix, the
  endpoint is a user-owned runtime/cache socket path such as
  `$HOME/.cache/synrepo-run/<hash>.sock`, falling back to runtime or temp dirs.
  On Windows, the endpoint is a named pipe `synrepo-watch-<hash>`.
- `openspec/` - planning artifacts only, not runtime.

## In-memory snapshot

- `GraphReader` is implemented by both `SqliteGraphStore` and immutable
  in-memory `Graph`.
- `src/structure/graph/snapshot.rs` stores process-global snapshots keyed by
  canonical repo root in a `LazyLock<RwLock<HashMap<PathBuf, Arc<Graph>>>>`.
  Publishing replaces one repo's entry without affecting other repos.
- Read-only MCP and card paths prefer the per-repo snapshot when available and
  fall back to SQLite otherwise.
- Stage 8 publishes a fresh snapshot after the structural SQLite transaction
  commits and drift scoring completes, so readers never observe partial graph
  state.
- `Config.max_graph_snapshot_bytes` defaults to `128 MiB`. Oversized snapshots
  are not published. Setting it to `0` disables publication entirely.

## Workspace layout

`Cargo.toml` is a single-member workspace containing the library and `synrepo`
binary. The MCP server runs as the `synrepo mcp` subcommand; there is no
separate binary crate.

`rmcp` is used by the binary-side MCP runtime. `tokio` also backs async
commentary/provider paths, and `schemars` derives JSON schemas for MCP and
overlay-facing parameter types. The library exposes mostly synchronous
CLI-facing entrypoints, but it is not strictly async-free.

## Node ID display format

`file_00000000000000000000000000000042`,
`sym_00000000000000000000000000000024`,
`concept_00000000000000000000000000000099`, and `edge_...` are 128-bit
blake3-derived IDs serialized as prefixed 32-character hexadecimal strings.
Do not cast them as raw integers through serde.

## Validation notes

- Import purity is intentionally not documented as a hard rule because current
  substrate code imports Git discovery helpers and optional embedding code reads
  graph snapshots for chunk extraction.
- Snapshot publication is documented as a per-repo `RwLock<HashMap<...>>`, not
  as a single global swap handle, because that is the current implementation in
  `src/structure/graph/snapshot.rs`.
- `.synrepo/config.toml` is documented as ignored because bootstrap writes
  `.synrepo/.gitignore` with only `!.gitignore`; tests assert that
  `!config.toml` is absent.
- Overlay is documented as active because the store currently implements
  commentary, proposed links, agent notes, lifecycle transitions, audit rows,
  read snapshots, and compaction.
