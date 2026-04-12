# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

> `CLAUDE.md` is a symlink to `AGENTS.md`. Edit `AGENTS.md`; both update.

## CI / Release

Workflows live in `.github/workflows/`: `ci.yml` (push/PR) and `release.yml` (tag trigger).

Secrets required in **this repo only** (Settings > Secrets and variables > Actions):
- `CARGO_REGISTRY_TOKEN` — crates.io token (scopes: publish-new, publish-update)
- `HOMEBREW_TAP_TOKEN` — GitHub PAT with repo scope on `whit3rabbit/homebrew-tap`

Homebrew tap is a sibling repo at `../homebrew-tap/`; cask template is at `packaging/homebrew/Casks/synrepo.rb`.

**Gotcha:** macOS Intel runner is `macos-15-intel` (not `macos-13` — deprecated Dec 2025).

## Commands

```bash
cargo build                        # build
cargo test                         # run all tests
cargo test <test_name>             # run a single test (substring match)
cargo test -p synrepo <test_name>  # run a single test by exact path
cargo clippy --workspace --all-targets -- -D warnings  # lint (CI-equivalent; covers MCP + test targets)
cargo fmt                          # format
make check                         # fmt-check + lint + test (CI equivalent)
cargo run -- init                  # initialize .synrepo/ in cwd
cargo run -- [--repo <path>] init  # override repo root
cargo run -- reconcile             # refresh graph store without full re-bootstrap
cargo run -- check                 # read-only drift report: surfaces, severities, recommended actions
cargo run -- check --json          # machine-readable JSON drift report
cargo run -- sync                  # repair auto-fixable drift surfaces; appends to .synrepo/state/repair-log.jsonl
cargo run -- sync --json           # JSON sync summary
cargo run -- status                # operational health: mode, counts, last reconcile, lock
cargo run -- agent-setup <tool>    # write integration shim for claude/cursor/copilot/generic
cargo run -- search <query>        # lexical search
cargo run -- graph query "outbound <node_id> [edge_kind]"  # graph traversal
cargo run -- graph stats           # node/edge counts
cargo run -- node <node_id>        # dump a node's metadata as JSON
RUST_LOG=debug cargo run -- <cmd>  # enable tracing output
```

Node IDs in display format: `file_0000000000000042`, `symbol_0000000000000024`, `concept_0000000000000099`.

Dev dependencies: `proptest` (property tests for token budget invariants), `insta` (snapshot tests for card output), `tempfile` (test fixtures). `criterion` is available for explicit benchmark work.

## Grep

Instead of grep or ripgrep use 'st' instead (syntext binary compatible with grep/rg)

## Architecture

Four layers, bottom to top. No layer may import from a layer above it.
Files must stay under 400 lines; split into sub-modules before they grow past that.

**0. Core** (`src/core/`) — Shared types with no heavy deps.
- `ids.rs` — stable identifier types: `FileNodeId`, `SymbolNodeId`, `ConceptNodeId`, `EdgeId`, `NodeId` (unified enum). These are the types named in the hard invariants below.
- `provenance.rs` — `Provenance`, `CreatedBy`, `SourceRef`: every graph row and overlay entry carries one.
- Spec: `openspec/specs/foundation/spec.md`

**1. Substrate** (`src/substrate/`) — File discovery, classification, and lexical index. Must not import from structure.
- `discover.rs` — filesystem walk via the `ignore` crate (respects `.gitignore`); produces `DiscoveredFile`
- `classify.rs` — maps files to `FileClass` (SupportedCode { language }, TextCode, Markdown, Jupyter, Skipped)
- `index.rs` — wraps `syntext` for n-gram lexical indexing and search; builds/queries `.synrepo/index/`
- Spec: `openspec/specs/substrate/spec.md`

**2. Structure** (`src/structure/`) — The canonical graph of directly-observed facts only.
- `graph/` — node types (`FileNode`, `SymbolNode`, `ConceptNode`), `EdgeKind`, `SymbolKind`, `Epistemic` (invariant comment in `epistemic.rs`), `GraphStore` trait
- `parse.rs` — tree-sitter parsers for Rust, Python, TypeScript/TSX; extracts `ExtractedSymbol` and `ExtractedEdge` records
- `prose.rs` — markdown concept extractor; produces `ConceptNode` from human-authored files in concept directories
- `identity.rs` — rename detection scaffold (TODO phase-1)
- `drift.rs` — per-edge drift score scaffold (TODO phase-1)
- `rationale.rs` — inline `// DECISION:` marker extraction from code files; results stored on `FileNode.inline_decisions`; cannot produce `ConceptNode` (invariant 7)
- Spec: `openspec/specs/graph/spec.md`

Node types: `FileNode` (content-hash identity), `SymbolNode` (tree-sitter extracted), `ConceptNode` (only from human-authored markdown in configured dirs such as `docs/concepts/`, `docs/adr/`; synthesis cannot create these).

**3. Overlay** (`src/overlay/mod.rs`) — LLM-authored content in a physically separate SQLite database from the graph. Defines `OverlayStore`, `OverlayLink`, `OverlayEpistemic` (`machine_authored_high_conf` | `machine_authored_low_conf`), `CitedSpan`. Phase 4+ only; the module exists to establish the architectural boundary from the start.
- Spec: `openspec/specs/overlay/spec.md`

**4. Surface** (`src/surface/`, `src/bin/cli.rs`) — CLI (phase 0/1), MCP server (phase 2+), skill bundle (`skill/SKILL.md`). `src/surface/card/mod.rs` is the stable card surface (`Budget`, `SymbolCard`, `FileCard`, `CardCompiler`, `Freshness`, `SourceStore`) with `git.rs` for Git projections, `types.rs` for card payload structs, `compiler/` for `GraphCardCompiler` (split into file.rs, io.rs, mod.rs, resolve.rs, symbol.rs), and `decision.rs` for `DecisionCard`.
- Spec: `openspec/specs/cards/spec.md`, `openspec/specs/mcp-surface/spec.md`

**Bootstrap** (`src/bootstrap/`) — First-run UX, mode detection, health checks. `src/bin/cli.rs` is a thin dispatcher only; all logic lives here.
- `init/` — `bootstrap()` orchestrator: builds substrate index, runs structural compile, writes config and snapshot
- `report.rs` — `BootstrapReport`, `BootstrapHealth`, `BootstrapAction`
- `mode_inspect.rs` — auto vs curated mode detection via `inspect_repository_mode()`
- Spec: `openspec/specs/bootstrap/spec.md`

**Pipeline** (`src/pipeline/`) — `structural/` defines the 8-stage compile cycle. `mod.rs` owns transaction orchestration and `CompileSummary`; `stages.rs` owns stages 1–3 (discover → parse code → parse prose); `stage4.rs` owns cross-file edge resolution. Stage 5 (git mining) runs via `src/pipeline/git/` and `src/pipeline/git_intelligence/`. Stages 6 (identity cascade, partially wired), 7 (drift scoring), and 8 (ArcSwap commit) are not yet wired end-to-end. `synthesis/` defines the `CommentaryGenerator` trait boundary with `stub.rs` (`NoOpGenerator`, default) and `claude.rs` (`ClaudeCommentaryGenerator`, reads `SYNREPO_ANTHROPIC_API_KEY`); called lazily by the card compiler at `Deep` budget when no overlay entry exists.
- `maintenance.rs` — storage-compatibility cleanup and compaction hooks; driven by `sync`.
- `repair/` — `mod.rs` is a thin façade. `report.rs` builds the read-only drift view, `sync.rs` drives auto-repair, `log.rs` appends JSONL resolution records, `declared_links.rs` verifies `Governs` targets, `commentary.rs` is the commentary-refresh repair action that calls the synthesis generator, and `types/` holds the stable enums plus report/log payload types.
- `git_intelligence/` — `mod.rs` is a thin façade. `types.rs` defines the public Git-intelligence payloads, `analysis.rs` derives history/hotspot/ownership/co-change summaries, and `tests/` is split by status, history, path, and shared support helpers.
- `watch.rs` — reconcile backstop and watch loop production logic; tests live in `src/pipeline/watch/tests.rs`.
- `writer.rs` — single-writer lock production logic; tests live in `src/pipeline/writer/tests.rs`.
- Spec: `openspec/specs/foundation/spec.md`

**Store** (`src/store/`) — SQLite backends implementing graph/overlay traits.
- `sqlite/` — `SqliteGraphStore`: opens/creates `.synrepo/graph/nodes.db`; implements `GraphStore`; provides `persisted_stats()` for CLI
- `compatibility/` — runtime layout checks, store versioning, migration/rebuild policy (`types.rs`, `evaluate/`, `snapshot.rs`)
- Spec: `openspec/specs/storage-and-compatibility/spec.md`

**Storage layout:**
- `.synrepo/graph/nodes.db` — canonical SQLite graph store (the file is named `nodes.db`)
- `.synrepo/overlay/` — overlay SQLite store (never mixed with graph)
- `.synrepo/index/` — syntext lexical index
- `.synrepo/config.toml` — runtime config (`Config` struct in `src/config.rs`)
- `.synrepo/.gitignore` — gitignores everything in `.synrepo/` except `config.toml` and `.gitignore`
- `.synrepo/state/writer.lock` — process-level write lock (PID + timestamp); held during `init`, `reconcile`, and `sync`
- `.synrepo/state/reconcile-state.json` — last reconcile outcome, timestamp, and discovered/symbol counts
- `.synrepo/state/repair-log.jsonl` — append-only resolution log written by `synrepo sync`; one JSON object per line
- `openspec/` — planning artifacts only, not runtime

### Layer and size rules

- No layer may import from a layer above it. Substrate must not import from structure.
- Every `.rs` file must stay under 400 lines. Split into a sub-module directory before exceeding that limit.

### Workspace layout

`Cargo.toml` is a workspace root with two members: `.` (the library + `synrepo` binary) and `crates/synrepo-mcp/` (the MCP server binary). The MCP crate adds `rmcp` and `tokio` without infecting the library.

## Hard invariants

These must hold across all changes:

1. `graph::Epistemic` has three variants: `ParserObserved`, `HumanDeclared`, `GitObserved`. Machine-authored content uses `overlay::OverlayEpistemic` instead. The type boundary is enforced by the type system — do not add machine variants to `Epistemic`.
2. The synthesis pipeline queries the graph with `source_store = "graph"` filtered at the retrieval layer. It never reads overlay output as input. This is structural, not just labeled.
3. `FileNodeId` is stable across renames. For new files it is derived from the content hash of the first-seen version (`derive_file_id` in `pipeline/structural/ids.rs`). For existing files the stored ID is always reused. Content-hash rename detection (stage 6) is implemented: a file moved to a new path with identical content preserves its `FileNodeId` and records the old path in `path_history`. Do not derive `FileNodeId` from path.
4. `ConceptNodeId` is path-derived (`derive_concept_id` in `structure/prose.rs`), making it stable across content edits but not renames. This differs from `FileNodeId` — do not confuse the two.
5. `SymbolNodeId` is keyed on `(file_node_id, qualified_name, kind, body_hash)`. A body rewrite changes the hash but keeps the node's graph slot via upsert.
6. `EdgeKind::Governs` is only created from human-authored frontmatter or inline `# DECISION:` markers, never inferred.
7. `ConceptNode` is only created from human-authored markdown in configured directories (`docs/concepts/`, `docs/adr/`, `docs/decisions/` by default). The synthesis pipeline cannot mint concept nodes in any mode.
8. Any multi-query read through a `GraphStore` or `OverlayStore` must run under `with_graph_read_snapshot` / `with_overlay_read_snapshot` (or the trait's `begin_read_snapshot`/`end_read_snapshot` pair). Without a snapshot, a writer commit between queries leaves the reader observing two committed epochs in one operation, which is how cards end up citing nodes and edges from different generations.

## Phase status

### Structural pipeline stage status

Stages 1–3 run on every `synrepo init`:
1. **Discover** — substrate walk, `.gitignore`/`.synignore` respected
2. **Parse code** — tree-sitter symbol extraction; emits `FileNode`, `SymbolNode`, `Defines` edges
3. **Parse prose** — concept node extraction from configured markdown directories

Stages 4–8:
4. Cross-file edge resolution (`calls`, `imports`, `inherits`, `references`) — **implemented** in `cards-and-mcp-v1`: name-based approximate resolution via tree-sitter call/import queries + post-parse name lookup pass in `src/pipeline/structural/stage4.rs`
5. Git mining (co-change, ownership, hotspots, recent file history) — **implemented** in `git-intelligence-v1`: deterministic first-parent history sampling via `src/pipeline/git/` and `src/pipeline/git_intelligence/`, surfaced today through file-facing outputs and node inspection
6. Identity cascade (rename detection) — **partially implemented**: content-hash based rename detection wired; split/merge detection still TODO
7. Drift scoring — TODO stub
8. ArcSwap commit — TODO stub

### Not yet implemented

- `synrepo watch` subcommand (`run_watch_loop` in `pipeline/watch.rs` is wired; no CLI entry point yet)
- `EntryPointCard`, `CallPathCard`; specialist MCP tools (`synrepo_entrypoints`, `synrepo_call_path`, etc.). `ModuleCard` is implemented (`src/surface/card/types.rs`).
- Graph-level `CoChangesWith` edges; symbol-level `SymbolCard.last_change`
- Drift scoring (stage 7), ArcSwap commit (stage 8), synthesis pipeline (phase 4+)

## Gotchas

- **`src/structure/parse/extract/` is a sub-module directory** (`mod.rs` ~388 lines, `qualname.rs` ~59 lines) — do not add more code to `mod.rs` without splitting further. Current watchlist (sorted): `src/pipeline/git/mod.rs` (357, approaching the 400 limit), `src/pipeline/structural/stages.rs` (345), `src/structure/prose.rs` (324), `src/store/compatibility/evaluate/mod.rs` (312), `src/pipeline/diagnostics.rs` (306). Re-check before adding to any of them.
- **`signature` and `doc_comment` are populated** by `src/structure/parse/extract/mod.rs` for Rust (`///` line comments, declaration up to `{`/`;`), Python (docstring, `def` line up to `:`), and TypeScript/TSX (JSDoc `/** */`, declaration up to `{`). These fields are safe to use in all three languages.
- **Stage 4 cross-file edges are now emitted**: `Calls` (file→symbol, approximate name resolution) and `Imports` (file→file, relative path resolution) edges are produced by `run_structural_compile`. `Inherits`, `References`, `CoChangesWith`, `Mentions` are not yet emitted. `SplitFrom` and `MergedFrom` edge kinds are defined but not yet produced.
- **`criterion` is present in `Cargo.toml`**, but the documented test workflow still centers on `proptest` and `insta`. Use `criterion` only for explicit benchmark work.
- **`.synrepo/graph/nodes.db`** is the actual SQLite file. Code that opens the graph store uses `SqliteGraphStore::open(&graph_dir)` where `graph_dir` is `.synrepo/graph/`; the `nodes.db` name is internal to `src/store/sqlite/mod.rs`.
- **Compatibility blocks on version mismatch**: if `.synrepo/` contains a graph store whose recorded format version is newer than the current binary understands, `synrepo init` and all graph commands will error. Resolve by removing `.synrepo/` and reinitializing.
- **Git history mining uses `gix`** (not `git2`). The current slice ships deterministic first-parent history sampling, degraded-history handling, hotspots, ownership hints, and file-scoped co-change summaries. Graph-level `CoChangesWith` edges and symbol-level last-change summaries are still future work.
- **`notify` and `notify-debouncer-full` are in `Cargo.toml`** and are used by `run_watch_loop` in `pipeline/watch.rs`. The watcher is implemented; there is no `synrepo watch` CLI subcommand yet.
- **`concept_directories` config defaults**: `docs/concepts`, `docs/adr`, `docs/decisions`. Adding a fourth directory (e.g. `architecture/decisions`) requires a config-sensitive compatibility check — changing this field triggers a graph advisory in the compat report.
- **File rename detection is implemented (content-hash matching).** When a file is moved to a new path with the same content, the structural compile detects the rename, preserves the `FileNodeId`, and records the old path in `path_history`. Caveat: split/merge detection is still TODO — a single file split into two will still produce orphaned nodes until split detection is wired.
- **Writer lock is enforced on all writes**: `synrepo init` and `synrepo reconcile` both acquire `.synrepo/state/writer.lock` before any state mutation. If a concurrent process holds the lock, both commands fail immediately with "writer lock held by pid N." Remove the lock file only if the recorded PID is confirmed dead (`kill -0 <pid>` returns non-zero). The canonical write path is `run_reconcile_pass()` in `pipeline/watch.rs` — any new code that needs to trigger a structural compile should go through it.
- **`repair/types/` has dual string mappings**: `RepairSurface`, `DriftClass`, `Severity`, and `RepairAction` each have `#[serde(rename_all = "snake_case")]` AND a manual `as_str()` in `src/pipeline/repair/types/stable.rs`. Adding a new variant requires updating both. The stable-identifier tests in `src/pipeline/repair/types/tests.rs` catch `as_str()` divergence from literals but do not cross-check serde output.
- **Structural compile is a single atomic transaction (stages 1–4)**: `run_structural_compile` wraps all four stages in one `BEGIN`/`COMMIT`. Stage 4 reads uncommitted nodes from stages 1–3 via SQLite read-your-own-writes on the same connection. The `with_transaction` helper that existed in `structural/mod.rs` has been removed; do not re-add it.
- **Reader snapshots are re-entrant**: `SqliteGraphStore::begin_read_snapshot` and the overlay equivalent use a `Mutex<usize>` depth counter. Only the outermost begin issues `BEGIN DEFERRED`; only the outermost end issues `COMMIT`. This lets an MCP handler wrap a request while `GraphCardCompiler` also wraps each method internally without tripping SQLite's "transaction within a transaction" error. Writer-side `begin`/`commit`/`rollback` is a separate lane (`&mut self`) and must not interleave with a read snapshot on the same handle. Note: `BEGIN DEFERRED` only upgrades to a real read transaction on the first SELECT, so the snapshot epoch is pinned at the first read, not at begin.
- **Both SQLite stores set `busy_timeout = 5000`** (see `src/store/sqlite/schema.rs` and `src/store/overlay/schema.rs`) so transient WAL checkpoint contention waits up to 5 s rather than surfacing `SQLITE_BUSY`. This becomes load-bearing when readers hold snapshots across writer commits.
- **`cargo build --workspace` does not imply `cargo test` will compile**: test-scoped code (`#[cfg(test)]` and `mod tests`) only compiles under `cargo test` / `cargo check --tests` / `cargo clippy --all-targets`. A pre-existing test-only compile error in an unrelated module will surface there, not in `cargo build`. When verifying focused work against in-tree WIP, isolate the WIP (temporary rename or stash) before running tests to confirm your own work.

## Config fields (`src/config.rs`)

| Field | Default | Notes |
|-------|---------|-------|
| `mode` | `auto` | `auto` or `curated` |
| `roots` | `["."]` | Roots to index, relative to repo root |
| `concept_directories` | `["docs/concepts", "docs/adr", "docs/decisions"]` | Concept/ADR dirs; changing triggers compat advisory |
| `git_commit_depth` | `500` | History depth budget for deterministic Git-intelligence sampling and file-scoped summaries |
| `max_file_size_bytes` | `1048576` (1 MB) | Files larger than this are skipped |
| `redact_globs` | `["**/secrets/**", "**/*.env*", "**/*-private.md"]` | Files matching these are never indexed |

## Reference docs

- `docs/FOUNDATION.md` — full foundational design: architecture, trust model, data model, pipelines, cross-linking, operational requirements, evaluation
- `docs/FOUNDATION-SPEC.md` — product spec: card types, budget tiers, MCP tool surface, phased build plan, acceptance target

## OpenSpec workflow

`openspec/specs/` holds enduring domain specs (stable intended behavior). `openspec/changes/<name>/` holds active work: `proposal.md`, `design.md`, `tasks.md`, and optional delta specs.

Active changes: none

Archived completed changes are in `openspec/changes/archive/`.

Specs govern intent; the graph governs runtime truth.
