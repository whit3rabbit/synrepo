# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
cargo build                        # build
cargo test                         # run all tests
cargo test <test_name>             # run a single test (substring match)
cargo test -p synrepo <test_name>  # run a single test by exact path
cargo clippy -- -D warnings        # lint (CI-equivalent)
cargo fmt                          # format
make check                         # fmt-check + lint + test (CI equivalent)
cargo run -- init                  # initialize .synrepo/ in cwd
cargo run -- [--repo <path>] init  # override repo root
cargo run -- reconcile             # refresh graph store without full re-bootstrap
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
- Spec: `openspec/specs/graph/spec.md`

Node types: `FileNode` (content-hash identity), `SymbolNode` (tree-sitter extracted), `ConceptNode` (only from human-authored markdown in configured dirs such as `docs/concepts/`, `docs/adr/`; synthesis cannot create these).

**3. Overlay** (`src/overlay/mod.rs`) — LLM-authored content in a physically separate SQLite database from the graph. Defines `OverlayStore`, `OverlayLink`, `OverlayEpistemic` (`machine_authored_high_conf` | `machine_authored_low_conf`), `CitedSpan`. Phase 4+ only; the module exists to establish the architectural boundary from the start.
- Spec: `openspec/specs/overlay/spec.md`

**4. Surface** (`src/surface/`, `src/bin/cli.rs`) — CLI (phase 0/1), MCP server (phase 2+), skill bundle (`skill/SKILL.md`). `card.rs` defines `Budget`, `SymbolCard`, `FileCard`, `CardCompiler` trait, `Freshness`, `SourceStore` for phase 2+ commands.
- Spec: `openspec/specs/cards/spec.md`, `openspec/specs/mcp-surface/spec.md`

**Bootstrap** (`src/bootstrap/`) — First-run UX, mode detection, health checks. `src/bin/cli.rs` is a thin dispatcher only; all logic lives here.
- `init/` — `bootstrap()` orchestrator: builds substrate index, runs structural compile, writes config and snapshot
- `report.rs` — `BootstrapReport`, `BootstrapHealth`, `BootstrapAction`
- `mode_inspect.rs` — auto vs curated mode detection via `inspect_repository_mode()`
- Spec: `openspec/specs/bootstrap/spec.md`

**Pipeline** (`src/pipeline/`) — `structural/` defines the 8-stage compile cycle. Currently stages 1–3 are implemented (discover → parse code → parse prose). Stages 4–8 (cross-file edge resolution, git mining, identity cascade, drift scoring, ArcSwap commit) are TODO stubs. `synthesis.rs` is a 4-line stub placeholder for the LLM pipeline (phase 4+).
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
- `.synrepo/state/writer.lock` — process-level write lock (PID + timestamp); held during `init` and `reconcile`
- `.synrepo/state/reconcile-state.json` — last reconcile outcome, timestamp, and discovered/symbol counts
- `openspec/` — planning artifacts only, not runtime

### Spec-to-module quick reference

| Module | Governing spec |
|--------|----------------|
| `src/core/` | `openspec/specs/foundation/spec.md` |
| `src/substrate/` | `openspec/specs/substrate/spec.md` |
| `src/structure/` | `openspec/specs/graph/spec.md` |
| `src/overlay/` | `openspec/specs/overlay/spec.md` |
| `src/store/compatibility/` | `openspec/specs/storage-and-compatibility/spec.md` |
| `src/surface/card/` | `openspec/specs/cards/spec.md` |
| `src/surface/mcp/` | `openspec/specs/mcp-surface/spec.md` |
| `src/bootstrap/` | `openspec/specs/bootstrap/spec.md` |
| `src/pipeline/` | `openspec/specs/foundation/spec.md` |

### Layer and size rules

- No layer may import from a layer above it. Substrate must not import from structure.
- Every `.rs` file must stay under 400 lines. Split into a sub-module directory before exceeding that limit.

### Workspace conversion

Milestone 2 is complete. The workspace conversion happens in `cards-and-mcp-v1` (Milestone 3): add a `[workspace]` section to the existing `Cargo.toml` and add `crates/synrepo-mcp/` as a new member. No files move. The library crate stays sync at the repo root; the MCP server binary lives in `crates/synrepo-mcp/` and adds `rmcp` (crates.io, modelcontextprotocol/rust-sdk) plus `tokio` as its own deps without infecting the library.

## Hard invariants

These must hold across all changes:

1. `graph::Epistemic` has three variants: `ParserObserved`, `HumanDeclared`, `GitObserved`. Machine-authored content uses `overlay::OverlayEpistemic` instead. The type boundary is enforced by the type system — do not add machine variants to `Epistemic`.
2. The synthesis pipeline queries the graph with `source_store = "graph"` filtered at the retrieval layer. It never reads overlay output as input. This is structural, not just labeled.
3. `FileNodeId` is stable across renames. For new files it is derived from the content hash of the first-seen version (`derive_file_id` in `pipeline/structural/ids.rs`). For existing files the stored ID is always reused. Content-hash rename detection (stage 6) is implemented: a file moved to a new path with identical content preserves its `FileNodeId` and records the old path in `path_history`. Do not derive `FileNodeId` from path.
4. `ConceptNodeId` is path-derived (`derive_concept_id` in `structure/prose.rs`), making it stable across content edits but not renames. This differs from `FileNodeId` — do not confuse the two.
5. `SymbolNodeId` is keyed on `(file_node_id, qualified_name, kind, body_hash)`. A body rewrite changes the hash but keeps the node's graph slot via upsert.
6. `EdgeKind::Governs` is only created from human-authored frontmatter or inline `# DECISION:` markers, never inferred.
7. `ConceptNode` is only created from human-authored markdown in configured directories (`docs/concepts/`, `docs/adr/`, `docs/decisions/` by default). The synthesis pipeline cannot mint concept nodes in any mode.

## Phase status

### Currently wired end-to-end

- `synrepo init` — idempotent bootstrap: creates on first run, refreshes on re-run, repairs if layout is partial. Auto-selects `auto` vs `curated` mode by scanning `concept_directories` for markdown; `--mode` overrides. Runs structural compile (stages 1–3) automatically, populating the graph with file nodes, symbol nodes, and concept nodes.
- `synrepo reconcile` — runs `run_reconcile_pass()` (same path as the watch loop): acquires writer lock, opens graph store, runs structural compile stages 1–3. Persists outcome to `.synrepo/state/reconcile-state.json`. Does not re-index the substrate or rewrite config.
- `synrepo status` — read-only operational health: mode, graph node counts, last reconcile outcome, writer lock state. Never acquires the writer lock. Safe to run while a reconcile is in progress.
- `synrepo agent-setup <tool>` — generates a thin integration shim for `claude`, `cursor`, `copilot`, or `generic`. Writes a named fragment file and prints the one-line include instruction. `--force` overwrites an existing file. Logic in `src/bin/cli_support/agent_shims.rs`.
- `synrepo search <query>` — calls `substrate::search` via syntext
- `synrepo graph query "<direction> <node_id> [edge_kind]"` — graph traversal; direction is `inbound` or `outbound`; edge_kind filter is optional
- `synrepo graph stats` — node and edge counts as JSON
- `synrepo node <id>` — dumps a node's metadata as JSON

### Structural pipeline stage status

Stages 1–3 run on every `synrepo init`:
1. **Discover** — substrate walk, `.gitignore`/`.synignore` respected
2. **Parse code** — tree-sitter symbol extraction; emits `FileNode`, `SymbolNode`, `Defines` edges
3. **Parse prose** — concept node extraction from configured markdown directories

Stages 4–8:
4. Cross-file edge resolution (`calls`, `imports`, `inherits`, `references`) — **implemented** in `cards-and-mcp-v1`: name-based approximate resolution via tree-sitter call/import queries + post-parse name lookup pass in `src/pipeline/structural/stage4.rs`
5. Git mining (co-change, ownership, blame) — planned for `git-intelligence-v1` using existing `GitIntelligenceContext`
6. Identity cascade (rename detection) — **partially implemented**: content-hash based rename detection wired; split/merge detection still TODO
7. Drift scoring — TODO stub
8. ArcSwap commit — TODO stub

### Not yet implemented

- `synrepo watch` CLI command (`run_watch_loop` in `pipeline/watch.rs` is implemented but not wired to a CLI subcommand; `synrepo reconcile` is the one-shot path)
- `ModuleCard`, `EntryPointCard`, `CallPathCard` and specialist MCP tools (`synrepo_entrypoints`, `synrepo_call_path`, `synrepo_test_surface`, `synrepo_minimum_context`, `synrepo_explain`, `synrepo_findings`) — next phases
- Stage 5 git mining (`CoChangesWith`, ownership, hotspots, last meaningful change) — next: `git-intelligence-v1`
- Synthesis pipeline (phase 4+)

## Gotchas

- **File size rule is currently satisfied**: no `src/**/*.rs` file should exceed 400 lines. Keep it that way by splitting modules before adding substantial new code.
- **`signature` and `doc_comment` are always `None`** until the phase-1 TODO in `src/structure/parse/extract.rs` is resolved. Do not write code that assumes these fields are populated.
- **Stage 4 cross-file edges are now emitted**: `Calls` (file→symbol, approximate name resolution) and `Imports` (file→file, relative path resolution) edges are produced by `run_structural_compile`. `Inherits`, `References`, `CoChangesWith`, `Mentions` are not yet emitted. `SplitFrom` and `MergedFrom` edge kinds are defined but not yet produced.
- **`criterion` is present in `Cargo.toml`**, but the documented test workflow still centers on `proptest` and `insta`. Use `criterion` only for explicit benchmark work.
- **`.synrepo/graph/nodes.db`** is the actual SQLite file. Code that opens the graph store uses `SqliteGraphStore::open(&graph_dir)` where `graph_dir` is `.synrepo/graph/`; the `nodes.db` name is internal to `src/store/sqlite/mod.rs`.
- **Compatibility blocks on version mismatch**: if `.synrepo/` contains a graph store whose recorded format version is newer than the current binary understands, `synrepo init` and all graph commands will error. Resolve by removing `.synrepo/` and reinitializing.
- **Git history mining uses `gix`** (not `git2`). The `gix` dep is included but git mining stages are TODO.
- **`notify` and `notify-debouncer-full` are in `Cargo.toml`** and are used by `run_watch_loop` in `pipeline/watch.rs`. The watcher is implemented; there is no `synrepo watch` CLI subcommand yet.
- **`concept_directories` config defaults**: `docs/concepts`, `docs/adr`, `docs/decisions`. Adding a fourth directory (e.g. `architecture/decisions`) requires a config-sensitive compatibility check — changing this field triggers a graph advisory in the compat report.
- **File rename detection is implemented (content-hash matching).** When a file is moved to a new path with the same content, the structural compile detects the rename, preserves the `FileNodeId`, and records the old path in `path_history`. Caveat: split/merge detection is still TODO — a single file split into two will still produce orphaned nodes until split detection is wired.
- **Writer lock is enforced on all writes**: `synrepo init` and `synrepo reconcile` both acquire `.synrepo/state/writer.lock` before any state mutation. If a concurrent process holds the lock, both commands fail immediately with "writer lock held by pid N." Remove the lock file only if the recorded PID is confirmed dead (`kill -0 <pid>` returns non-zero). The canonical write path is `run_reconcile_pass()` in `pipeline/watch.rs` — any new code that needs to trigger a structural compile should go through it.

## Config fields (`src/config.rs`)

| Field | Default | Notes |
|-------|---------|-------|
| `mode` | `auto` | `auto` or `curated` |
| `roots` | `["."]` | Roots to index, relative to repo root |
| `concept_directories` | `["docs/concepts", "docs/adr", "docs/decisions"]` | Concept/ADR dirs; changing triggers compat advisory |
| `git_commit_depth` | `500` | History depth for git mining (stage 5, not yet active) |
| `max_file_size_bytes` | `1048576` (1 MB) | Files larger than this are skipped |
| `redact_globs` | `["**/secrets/**", "**/*.env*", "**/*-private.md"]` | Files matching these are never indexed |

## Reference docs

- `docs/FOUNDATION.md` — full foundational design: architecture, trust model, data model, pipelines, cross-linking, operational requirements, evaluation
- `docs/FOUNDATION-SPEC.md` — product spec: card types, budget tiers, MCP tool surface, phased build plan, acceptance target

## OpenSpec workflow

`openspec/specs/` holds enduring domain specs (stable intended behavior). `openspec/changes/<name>/` holds active work: `proposal.md`, `design.md`, `tasks.md`, and optional delta specs.

Active changes:
- `git-intelligence-v1` — stage 5 git mining, CoChangesWith edges, card enrichment (proposal/design/tasks exist; implement after cards-and-mcp-v1)
- `storage-compatibility-v1`, `lexical-substrate-v1`, `bootstrap-ux-v1`, `foundation-bootstrap` — earlier change artifacts (partially or fully implemented)

Archived: `openspec/changes/archive/` — completed changes including `2026-04-10-structural-graph-v1` (structural compile pipeline), `2026-04-11-watch-reconcile-v1` (watcher, reconcile, single-writer lock), `2026-04-11-agent-integration-v1` (status command, agent-setup shims, skill/SKILL.md current-phase section), and `cards-and-mcp-v1` (stage 4 edges, CardCompiler, workspace conversion, MCP server with 5 core tools).

Specs govern intent; the graph governs runtime truth.
