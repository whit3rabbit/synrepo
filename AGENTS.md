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
cargo run -- search <query>        # lexical search
RUST_LOG=debug cargo run -- <cmd>  # enable tracing output
```

Dev dependencies: `proptest` (property tests for token budget invariants), `insta` (snapshot tests for card output), `criterion` (benchmarks), `tempfile` (test fixtures).

## Architecture

Four layers, bottom to top. No layer may import from a layer above it.
Files must stay under 400 lines; split into sub-modules before they grow past that.

**0. Core** (`src/core/`) — Shared types with no heavy deps.
- `ids.rs` — stable identifier types: `FileNodeId`, `SymbolNodeId`, `ConceptNodeId`, `NodeId` (unified enum). These are the types named in the hard invariants below.
- `provenance.rs` — `Provenance`, `CreatedBy`, `SourceRef`: every graph row and overlay entry carries one.
- Spec: `openspec/specs/foundation/spec.md`

**1. Substrate** (`src/substrate/`) — File discovery, classification, and lexical index. Must not import from structure.
- `discover.rs` — filesystem walk via the `ignore` crate (respects `.gitignore`); produces `DiscoveredFile`
- `classify.rs` — maps files to `FileClass` (SupportedCode, TextCode, Markdown, Jupyter, Skipped)
- `index.rs` — wraps `syntext` for n-gram lexical indexing and search; builds/queries `.synrepo/index/`
- Spec: `openspec/specs/substrate/spec.md`

**2. Structure** (`src/structure/`) — The canonical graph of directly-observed facts only.
- `graph/` — node types (`FileNode`, `SymbolNode`), `EdgeKind`, `Epistemic` (invariant comment in `epistemic.rs`), `GraphStore` trait
- `parse.rs` — tree-sitter parsers for Rust, Python, TypeScript; markdown link parser
- `identity.rs` — rename detection: `FileNodeId` survives renames via AST-based cascade
- `drift.rs` — per-edge drift scores recomputed on every commit
- Spec: `openspec/specs/graph/spec.md`

Node types: `FileNode` (content-hash identity), `SymbolNode` (tree-sitter extracted), `ConceptNode` (only from human-authored markdown in configured dirs such as `docs/concepts/`, `docs/adr/`; synthesis cannot create these).

**3. Overlay** (`src/overlay/`) — LLM-authored content in a physically separate SQLite database from the graph. Defines `OverlayStore`, `OverlayLink`, `OverlayEpistemic` (`machine_authored_high_conf` | `machine_authored_low_conf`), `CitedSpan`. Phase 4+ only; the module exists to establish the architectural boundary from the start.
- Spec: `openspec/specs/overlay/spec.md`

**4. Surface** (`src/surface/`, `src/bin/cli.rs`) — CLI (phase 0/1), MCP server (phase 2+), skill bundle (`skill/SKILL.md`). `card.rs` defines card types returned by phase 2+ commands.
- Spec: `openspec/specs/cards/spec.md`, `openspec/specs/mcp-surface/spec.md`

**Bootstrap** (`src/bootstrap/`) — First-run UX, mode detection, health checks. `src/bin/cli.rs` is a thin dispatcher only; all logic lives here.
- `init.rs` — `bootstrap()` orchestrator, config loading, gitignore setup
- `report.rs` — `BootstrapReport`, `BootstrapHealth`, `BootstrapAction`
- `mode_inspect.rs` — auto vs curated mode detection via `inspect_repository_mode()`
- Spec: `openspec/specs/bootstrap/spec.md`

**Pipeline** (`src/pipeline/`) — `structural.rs` defines the 8-stage compile cycle (discover → parse code → parse prose → resolve cross-file edges → git mine → identity → drift → commit). `synthesis.rs` is the LLM-driven overlay pipeline (phase 4+). Both files exist as skeletons; stages are TODO(phase-0/1).
- Spec: `openspec/specs/foundation/spec.md`

**Store** (`src/store/`) — SQLite backends implementing graph/overlay traits.
- `compatibility/` — runtime layout checks, store versioning, migration/rebuild policy (`types.rs`, `evaluate.rs`, `snapshot.rs`)
- Phase 1: `graph/` sqlite implementation of `GraphStore`; `overlay/` phase-4 stubs
- Spec: `openspec/specs/storage-and-compatibility/spec.md`

**Storage layout:**
- `.synrepo/graph/` — canonical SQLite graph store
- `.synrepo/overlay/` — overlay SQLite store (never mixed with graph)
- `.synrepo/index/` — syntext lexical index
- `.synrepo/config.toml` — runtime config (`Config` struct in `src/config.rs`)
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
3. `FileNodeId` is stable across renames. Do not derive it from path — it is derived from content hash of the first-seen version. Path history is stored on `FileNode.path_history`.
4. `SymbolNodeId` is keyed on `(file_node_id, qualified_name, kind, body_hash)`. A body rewrite creates a new identity revision, not a new node.
5. `EdgeKind::Governs` is only created from human-authored frontmatter or inline `# DECISION:` markers, never inferred.
6. `ConceptNode` is only created from human-authored markdown in configured directories (`docs/concepts/`, `docs/adr/`, `docs/decisions/` by default). The synthesis pipeline cannot mint concept nodes in any mode.

## Phase status

Most of the codebase is architectural scaffolding with `TODO(phase-0)` and `TODO(phase-1)` markers. What is actually wired end-to-end:

- `synrepo init` — idempotent bootstrap: creates on first run, refreshes on re-run, repairs if layout is partial. Auto-selects `auto` vs `curated` mode by scanning `concept_directories` for markdown; `--mode` overrides.
- `synrepo search <query>` — calls `substrate::search` via syntext

Not yet implemented (bail with error): `synrepo graph query`, `synrepo graph stats`, `synrepo node`.

Card-returning commands and MCP server are phase 2.

## Reference docs

- `docs/FOUNDATION.md` — full foundational design: architecture, trust model, data model, pipelines, cross-linking, operational requirements, evaluation
- `docs/FOUNDATION-SPEC.md` — product spec: card types, budget tiers, MCP tool surface, phased build plan, acceptance target

## OpenSpec workflow

`openspec/specs/` holds enduring domain specs (stable intended behavior). `openspec/changes/<name>/` holds active work: `proposal.md`, `design.md`, `tasks.md`, and optional delta specs. Active changes: `foundation-bootstrap`, `bootstrap-ux-v1`, `lexical-substrate-v1`, `git-intelligence-v1`, `storage-compatibility-v1`. Specs govern intent; the graph governs runtime truth.
