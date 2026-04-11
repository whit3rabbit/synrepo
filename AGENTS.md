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

Stay single-crate through Milestone 2. Convert to workspace when the MCP server binary is wired (phase 2): the server has a different async dep profile that benefits from separate compilation.

## Hard invariants

These must hold across all changes:

1. `graph::Epistemic` has three variants: `ParserObserved`, `HumanDeclared`, `GitObserved`. Machine-authored content uses `overlay::OverlayEpistemic` instead. The type boundary is enforced by the type system — do not add machine variants to `Epistemic`.
2. The synthesis pipeline queries the graph with `source_store = "graph"` filtered at the retrieval layer. It never reads overlay output as input. This is structural, not just labeled.
3. `FileNodeId` is stable across renames. **This invariant is not yet enforced at runtime.** For new files it is derived from the content hash of the first-seen version (`derive_file_id` in `pipeline/structural/ids.rs`). For existing files the stored ID is always reused. Rename detection (stage 6) is not yet implemented: until it is, a rename will produce a new ID instead of preserving the old one. Do not derive `FileNodeId` from path.
4. `ConceptNodeId` is path-derived (`derive_concept_id` in `structure/prose.rs`), making it stable across content edits but not renames. This differs from `FileNodeId` — do not confuse the two.
5. `SymbolNodeId` is keyed on `(file_node_id, qualified_name, kind, body_hash)`. A body rewrite changes the hash but keeps the node's graph slot via upsert.
6. `EdgeKind::Governs` is only created from human-authored frontmatter or inline `# DECISION:` markers, never inferred.
7. `ConceptNode` is only created from human-authored markdown in configured directories (`docs/concepts/`, `docs/adr/`, `docs/decisions/` by default). The synthesis pipeline cannot mint concept nodes in any mode.

## Phase status

### Currently wired end-to-end

- `synrepo init` — idempotent bootstrap: creates on first run, refreshes on re-run, repairs if layout is partial. Auto-selects `auto` vs `curated` mode by scanning `concept_directories` for markdown; `--mode` overrides. Runs structural compile (stages 1–3) automatically, populating the graph with file nodes, symbol nodes, and concept nodes.
- `synrepo search <query>` — calls `substrate::search` via syntext
- `synrepo graph query "<direction> <node_id> [edge_kind]"` — graph traversal; direction is `inbound` or `outbound`; edge_kind filter is optional
- `synrepo graph stats` — node and edge counts as JSON
- `synrepo node <id>` — dumps a node's metadata as JSON

### Structural pipeline stage status

Stages 1–3 run on every `synrepo init`:
1. **Discover** — substrate walk, `.gitignore`/`.synignore` respected
2. **Parse code** — tree-sitter symbol extraction; emits `FileNode`, `SymbolNode`, `Defines` edges
3. **Parse prose** — concept node extraction from configured markdown directories

Stages 4–8 are TODO stubs:
4. Cross-file edge resolution (`calls`, `imports`, `inherits`, `references`) — not yet populated
5. Git mining (co-change, ownership, blame)
6. Identity cascade (rename detection)
7. Drift scoring
8. ArcSwap commit

### Not yet implemented

- Watcher / reconcile loop (next: `watch-reconcile-v1`)
- Card compilers (`CardCompiler` trait defined in `surface/card.rs` but not implemented)
- MCP server (phase 2)
- Synthesis pipeline (phase 4+)

## Gotchas

- **File size rule is currently satisfied**: no `src/**/*.rs` file should exceed 400 lines. Keep it that way by splitting modules before adding substantial new code.
- **`signature` and `doc_comment` are always `None`** until the phase-1 TODO in `src/structure/parse/extract.rs` is resolved. Do not write code that assumes these fields are populated.
- **Only `Defines` edges exist** in the graph currently. `Calls`, `Imports`, `Inherits`, `References`, `CoChangesWith`, `Mentions` edges are not emitted until stages 4–5 are implemented. `SplitFrom` and `MergedFrom` edge kinds are defined but not yet produced.
- **`criterion` is present in `Cargo.toml`**, but the documented test workflow still centers on `proptest` and `insta`. Use `criterion` only for explicit benchmark work.
- **`.synrepo/graph/nodes.db`** is the actual SQLite file. Code that opens the graph store uses `SqliteGraphStore::open(&graph_dir)` where `graph_dir` is `.synrepo/graph/`; the `nodes.db` name is internal to `src/store/sqlite/mod.rs`.
- **Compatibility blocks on version mismatch**: if `.synrepo/` contains a graph store whose recorded format version is newer than the current binary understands, `synrepo init` and all graph commands will error. Resolve by removing `.synrepo/` and reinitializing.
- **Git history mining uses `gix`** (not `git2`). The `gix` dep is included but git mining stages are TODO.
- **`notify` and `notify-debouncer-full` are in `Cargo.toml`** but the watcher loop is not implemented. They are placeholders for `watch-reconcile-v1`.
- **`concept_directories` config defaults**: `docs/concepts`, `docs/adr`, `docs/decisions`. Adding a fourth directory (e.g. `architecture/decisions`) requires a config-sensitive compatibility check — changing this field triggers a graph advisory in the compat report.
- **File renames are not yet detected.** Until the identity cascade (stage 6) is implemented, a file renamed from `src/old.rs` to `src/new.rs` creates a new `FileNodeId` for `src/new.rs` and leaves the old node orphaned until the next compile removes it. Any code that persists a `FileNodeId` across a rename will reference a stale node. Do not build external surfaces that cache node IDs until rename detection is wired.
- **`path_history` is always empty** until stage 6 is implemented. The field is defined on `FileNode` and persists correctly, but no compile pass populates it yet. Do not read it expecting populated values.

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

Active changes (not yet started):
- `watch-reconcile-v1` — single-writer watch and reconcile loop
- `git-intelligence-v1`, `storage-compatibility-v1`, `lexical-substrate-v1`, `bootstrap-ux-v1`, `foundation-bootstrap` — earlier change artifacts (partially or fully implemented)

Archived: `openspec/changes/archive/` — completed changes including `2026-04-10-structural-graph-v1` (the structural compile pipeline, now done).

Specs govern intent; the graph governs runtime truth.
