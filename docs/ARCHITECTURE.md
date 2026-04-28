# Architecture

Four layers, bottom to top. No layer may import from a layer above it.
Files must stay under 400 lines; split into sub-modules before they grow past that.

## Layers

**0. Core** (`src/core/`) — Shared types with no heavy deps.
- `ids.rs` — stable identifier types: `FileNodeId`, `SymbolNodeId`, `ConceptNodeId`, `EdgeId`, `NodeId` (unified enum). Backed by `u128` blake3 hashes. They implement `Serialize / Deserialize` to 32-char snake-case hexadecimal strings (e.g., `"file_..."`) to bypass serde_json maximum number limits (`u64::MAX`). SQLite columns for IDs are type `TEXT PRIMARY KEY`.
- `provenance.rs` — `Provenance`, `CreatedBy`, `SourceRef`: every graph row and overlay entry carries one.
- Spec: `openspec/specs/foundation/spec.md`

**1. Substrate** (`src/substrate/`) — File discovery, classification, and lexical index. Must not import from structure.
- `discover.rs` — filesystem walk via the `ignore` crate (respects `.gitignore`); produces `DiscoveredFile` from the primary checkout, linked worktrees when `include_worktrees = true`, and initialized submodules when `include_submodules = true`
- `classify.rs` — maps files to `FileClass` (SupportedCode { language }, TextCode, Markdown, Jupyter, Skipped)
- `index.rs` — wraps `syntext` for n-gram lexical indexing and search; builds/queries `.synrepo/index/`
- Spec: `openspec/specs/substrate/spec.md`

**2. Structure** (`src/structure/`) — The canonical graph of directly-observed facts only.
- `graph/` — node types (`FileNode`, `SymbolNode`, `ConceptNode`), `EdgeKind`, `SymbolKind`, `Epistemic` (invariant comment in `epistemic.rs`), `GraphStore` trait
- `parse/` — tree-sitter parsers for Rust, Python, TypeScript/TSX, and Go; extracts `ExtractedSymbol` and `ExtractedEdge` records (see `docs/ADDING-LANGUAGE.md`)
- `prose.rs` — markdown concept extractor; produces `ConceptNode` from human-authored files in concept directories
- `identity.rs` — rename detection cascade (5 steps: content-hash, split, merge, git rename, breakage)
- `drift.rs` — per-edge drift scoring via Jaccard distance on persisted structural fingerprints
- `rationale.rs` — inline `// DECISION:` marker extraction from code files; results stored on `FileNode.inline_decisions`; cannot produce `ConceptNode` (invariant 7)
- Spec: `openspec/specs/graph/spec.md`

Node types: `FileNode` (root-discriminated content-hash identity), `SymbolNode` (tree-sitter extracted), `ConceptNode` (only from human-authored markdown in configured dirs such as `docs/concepts/`, `docs/adr/`; explain cannot create these).

**3. Overlay** (`src/overlay/mod.rs`) — LLM-authored content in a physically separate SQLite database from the graph. Defines `OverlayStore`, `OverlayLink`, `OverlayEpistemic` (`machine_authored_high_conf` | `machine_authored_low_conf`), `CitedSpan`. Phase 4+ only; the module exists to establish the architectural boundary from the start.
- Spec: `openspec/specs/overlay/spec.md`

**4. Surface** (`src/surface/`, `src/bin/cli.rs`) — CLI (phase 0/1), MCP server (`synrepo mcp` subcommand, phase 2+), skill bundle (`skill/SKILL.md`). `src/surface/card/mod.rs` is the stable card surface (`Budget`, `SymbolCard`, `FileCard`, `CardCompiler`, `Freshness`, `SourceStore`) with `git.rs` for Git projections, `types.rs` for card payload structs, `compiler/` for `GraphCardCompiler` (split into file.rs, io.rs, mod.rs, resolve.rs, symbol.rs), and `decision.rs` for `DecisionCard`. `src/surface/mcp/` holds the MCP tool handlers (helpers, cards, search, audit, findings, primitives) and `SynrepoState`; the server dispatch lives in `src/bin/cli_support/commands/mcp.rs`.
- Spec: `openspec/specs/cards/spec.md`, `openspec/specs/mcp-surface/spec.md`

## Bootstrap

`src/bootstrap/` — First-run UX, mode detection, health checks. `src/bin/cli.rs` is a thin dispatcher only; all logic lives here.
- `init/` — `bootstrap()` orchestrator: builds substrate index, runs structural compile, writes config and snapshot
- `report.rs` — `BootstrapReport`, `BootstrapHealth`, `BootstrapAction`
- `mode_inspect.rs` — auto vs curated mode detection via `inspect_repository_mode()`
- Spec: `openspec/specs/bootstrap/spec.md`

`bootstrap()` signature: `synrepo::bootstrap::bootstrap(repo_root: &Path, mode: Option<Mode>)` — two args only. Does not accept a pre-built `Config` or `synrepo_dir`; it derives both internally.

## Pipeline

`src/pipeline/` — `structural/` defines the 8-stage compile cycle. `mod.rs` owns transaction orchestration and `CompileSummary`; `stages.rs` owns stages 1–3 (discover → parse code → parse prose); `stage4.rs` owns cross-file edge resolution. Stage 5 (git mining) runs via `src/pipeline/git/` and `src/pipeline/git_intelligence/`. Stage 6 (identity cascade: content-hash, split, merge, git rename fallback, breakage) is wired. Stage 7 (drift scoring via Jaccard distance on persisted structural fingerprints) is wired and writes to the sidecar `edge_drift` and `file_fingerprints` tables. Stage 8 publishes the immutable in-memory graph snapshot via `ArcSwap<Graph>` after the SQLite commit succeeds. `explain/` defines the `CommentaryGenerator` trait boundary with `stub.rs` (`NoOpGenerator`, default) and `providers/` for the configured explain backends; called explicitly via `synrepo_refresh_commentary` or sync repair actions.

- `maintenance.rs` — storage-compatibility cleanup and compaction hooks; driven by `sync`.
- `repair/` — `mod.rs` is a thin façade. `report/` holds the drift-report builder with `surfaces/` (10 `SurfaceCheck` implementations split into `mod.rs`, `commentary.rs`, `cross_links.rs`, `drift.rs`, `rationale.rs`). `sync.rs` drives auto-repair, `cross_links.rs` runs the cross-link generation pass, `log.rs` appends JSONL resolution records, `declared_links.rs` verifies `Governs` targets, `commentary.rs` is the commentary-refresh repair action that calls the explain generator, and `types/` holds the stable enums plus report/log payload types.
- `git_intelligence/` — `mod.rs` is a thin façade. `types.rs` defines the public Git-intelligence payloads, `analysis.rs` derives history/hotspot/ownership/co-change summaries, `emit.rs` emits `CoChangesWith` edges into the graph after each git pass, `symbol_revisions/` tracks per-symbol `first_seen_rev`/`last_modified_rev` via body-hash diffing, and `tests/` is split by status, history, path, and shared support helpers.
- `watch/` — reconcile backstop, watch lease/control plane, and watch loop production logic; tests live in `src/pipeline/watch/tests.rs`.
- `writer.rs` — single-writer lock production logic; tests live in `src/pipeline/writer/tests.rs`.
- Spec: `openspec/specs/foundation/spec.md`

### Structural stages

Structural parsing supports Rust, Python, TypeScript/TSX, and Go.

1. **Discover** — substrate walk, `.gitignore`/`.synignore` respected per discovery root. The root set is the primary checkout plus linked worktrees by default; initialized submodules are opt-in and walked as separate roots.
2. **Parse code** — tree-sitter symbol extraction; emits `FileNode`, `SymbolNode`, `Defines` edges.
3. **Parse prose** — concept node extraction from configured markdown directories.
4. **Cross-file edge resolution** — emits `Calls` (file→symbol for transition and module-scope calls, symbol→symbol for calls inside an extracted caller symbol; scoped resolution: +100 same-file, +50 imported, +20/+10/-100 visibility, +30 kind, +40 prefix. Edges only > 0 AND (unique OR tied ≥50)) and `Imports` (file→file). Resolvers: TypeScript/TSX (relative path), Python (dotted module), Rust (`crate::`/`self::`/`super::` plus bare top-level crate paths with longest-match selection), Go (module-prefix stripping via `go.mod`, fanning out to every `.go` file in the target package). `Inherits`, `References`, `Mentions` are not yet emitted.
5. **Git mining** — deterministic first-parent history sampling per discovery root; emits `CoChangesWith` edges via `git_intelligence/emit.rs`. Cross-root Git edges are not emitted.
6. **Identity cascade** — content-hash rename, split/merge detection, git rename fallback; emits `SplitFrom` / `MergedFrom`. Preserves `FileNodeId` across renames and records old paths in `path_history`.
7. **Drift scoring** — Jaccard distance on persisted structural fingerprints; writes sidecar `edge_drift` and `file_fingerprints` tables.
8. **ArcSwap publish** — rebuild immutable `Graph` from SQLite, then atomically publish via `ArcSwap`.

Stages 1–4 run in a single atomic SQLite transaction (`run_structural_compile`). Stage 4 reads uncommitted nodes from stages 1–3 via read-your-own-writes on the same connection. Watch-triggered reconciles can scope stages 1–6 to the discovery root that owns the changed path, leaving sibling roots untouched.

#### Body-Scope Calls

Call extraction carries the enclosing caller symbol's `(qualified_name, body_hash)` pair. Stage 4 resolves that pair against the active symbols from the same transaction and emits a symbol-to-symbol `Calls` edge when the callee scores to a positive candidate. If no enclosing caller exists, for example a module-top-level statement, stage 4 keeps the file-to-symbol `Calls` edge only.

### Overlay and audit surfaces

- Cross-link overlay store, card surfacing, CLI review flow, `synrepo_findings` MCP audit tool, and `synrepo sync --generate-cross-links` / `--regenerate-cross-links` are implemented.
- Cross-link promotion remains curated-mode-only. Accepted links become graph edges with `Epistemic::HumanDeclared`; overlay candidates stay in the audit trail.

## Store

`src/store/` — SQLite backends implementing graph/overlay traits.
- `sqlite/` — `SqliteGraphStore`: opens/creates `.synrepo/graph/nodes.db`; implements `GraphStore`; provides `persisted_stats()` for CLI.
- `compatibility/` — runtime layout checks, store versioning, migration/rebuild policy (`types.rs`, `evaluate/`, `snapshot.rs`).
- Spec: `openspec/specs/storage-and-compatibility/spec.md`

## Storage layout

- `.synrepo/graph/nodes.db` — canonical SQLite graph store (symbols table includes `body_hash` column, indexed, alongside JSON blob)
- `.synrepo/overlay/` — overlay SQLite store (never mixed with graph)
- `.synrepo/index/` — syntext lexical index
- `.synrepo/config.toml` — runtime config (`Config` struct in `src/config/mod.rs`; see `docs/CONFIG.md`)
- `.synrepo/.gitignore` — gitignores everything in `.synrepo/` except `config.toml` and `.gitignore`
- `.synrepo/state/writer.lock` — process-level write lock (PID + timestamp); held during each actual runtime mutation, including watch-triggered reconcile passes
- `.synrepo/state/watch-daemon.json` — per-repo watch lease plus owner/telemetry snapshot for `synrepo watch`
- `.synrepo/state/watch-daemon.log` — stderr of the detached watch daemon; truncated on each spawn, useful for post-mortem on startup crashes
- `.synrepo/state/watch.sock` — Unix-only control socket for active daemon watch mode (on Windows the control plane is a named pipe `synrepo-watch-<hash>` with no on-disk artifact)
- `.synrepo/state/reconcile-state.json` — last reconcile outcome, timestamp, and discovered/symbol counts
- `.synrepo/state/repair-log.jsonl` — append-only resolution log written by `synrepo sync`; one JSON object per line
- `.synrepo/state/explain-log.jsonl` — append-only per-call explain telemetry (see `docs/EXPLAIN.md`)
- `.synrepo/state/explain-totals.json` — aggregates snapshot consumed by the Health tab; written transactionally (temp + rename) after each call
- `.synrepo/state/explain-scope.json` — folder-picker persisted selection for `synrepo explain` (UI state, not operator config); load failures fall back to the heuristic rather than crashing
- `openspec/` — planning artifacts only, not runtime

Code that opens the graph store uses `SqliteGraphStore::open(&graph_dir)` where `graph_dir` is `.synrepo/graph/`; the `nodes.db` name is internal to `src/store/sqlite/mod.rs`.

## In-memory snapshot

- `GraphReader` is the read-only graph trait implemented by both `SqliteGraphStore` and the immutable in-memory `Graph`.
- `src/structure/graph/snapshot.rs` holds the process-global `ArcSwap<Graph>` handle. Read-only MCP/card paths prefer this snapshot when available.
- Stage 8 publishes a fresh snapshot after the structural SQLite transaction commits and drift scoring completes, so readers never observe partial graph state.
- `Config.max_graph_snapshot_bytes` defaults to `128 MiB`. It is advisory: oversized snapshots still publish with a warning. Setting it to `0` disables publication and leaves readers on the SQLite path.

## Layer and size rules

- No layer may import from a layer above it. Substrate must not import from structure.
- Every `.rs` file must stay under 400 lines. Split into a sub-module directory before exceeding that limit.

## Workspace layout

`Cargo.toml` is a single-member workspace (the library + `synrepo` binary). The MCP server runs as a `synrepo mcp` subcommand — there is no separate binary crate. `rmcp`, `tokio`, and `schemars` are direct dependencies; `rmcp` and `tokio` are only used by the binary-side MCP command (`src/bin/cli_support/commands/mcp.rs`), keeping the library crate (`src/lib.rs`) synchronous.

## Node ID display format

`file_00000000000000000000000000000042`, `sym_00000000000000000000000000000024`, `concept_00000000000000000000000000000099`. These are 128-bit (16-byte) blake3 hashes serialized as 32-character hexadecimal strings to prevent JSON numerical parser limitations. Do not cast them as raw integers via serde.
