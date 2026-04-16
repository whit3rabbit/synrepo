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
cargo test --bin synrepo <test_name>  # run binary-crate tests (cli_support::tests::*)
cargo clippy --all-targets -- -D warnings  # lint (CI-equivalent)
cargo fmt                          # format
make check                         # fmt-check + lint + test (CI equivalent)
cargo run -- init                  # initialize .synrepo/ in cwd
cargo run -- [--repo <path>] init  # override repo root
cargo run -- reconcile             # refresh graph store without full re-bootstrap
cargo run -- check                 # read-only drift report: surfaces, severities, recommended actions
cargo run -- check --json          # machine-readable JSON drift report
cargo run -- sync                  # repair auto-fixable drift surfaces; appends to .synrepo/state/repair-log.jsonl
cargo run -- sync --json           # JSON sync summary
cargo run -- status [--json]        # operational health: mode, counts, last reconcile, lock, export freshness
cargo run -- export [--format markdown|json] [--deep] [--commit] [--out <dir>]  # generate synrepo-context/
cargo run -- upgrade [--apply]     # dry-run or apply storage compatibility actions
cargo run -- watch                 # foreground watch for the current repo
cargo run -- watch --daemon        # detached per-repo watch service
cargo run -- watch status          # watch ownership and reconcile telemetry
cargo run -- watch stop            # stop active watch service or clean stale watch artifacts
cargo run -- agent-setup <tool>    # write integration shim; tools: claude/cursor/copilot/generic/codex/windsurf; --regen to update if stale
cargo run -- search <query>        # lexical search
cargo run -- graph query "outbound <node_id> [edge_kind]"  # graph traversal
cargo run -- graph stats           # node/edge counts
cargo run -- node <node_id>        # dump a node's metadata as JSON
cargo run -- mcp                   # start MCP server over stdio
cargo run -- mcp --repo <path>     # start MCP server for a specific repo
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
- `identity.rs` — rename detection cascade (5 steps: content-hash, split, merge, git rename, breakage)
- `drift.rs` — per-edge drift scoring via Jaccard distance on persisted structural fingerprints
- `rationale.rs` — inline `// DECISION:` marker extraction from code files; results stored on `FileNode.inline_decisions`; cannot produce `ConceptNode` (invariant 7)
- Spec: `openspec/specs/graph/spec.md`

Node types: `FileNode` (content-hash identity), `SymbolNode` (tree-sitter extracted), `ConceptNode` (only from human-authored markdown in configured dirs such as `docs/concepts/`, `docs/adr/`; synthesis cannot create these).

**3. Overlay** (`src/overlay/mod.rs`) — LLM-authored content in a physically separate SQLite database from the graph. Defines `OverlayStore`, `OverlayLink`, `OverlayEpistemic` (`machine_authored_high_conf` | `machine_authored_low_conf`), `CitedSpan`. Phase 4+ only; the module exists to establish the architectural boundary from the start.
- Spec: `openspec/specs/overlay/spec.md`

**4. Surface** (`src/surface/`, `src/bin/cli.rs`) — CLI (phase 0/1), MCP server (`synrepo mcp` subcommand, phase 2+), skill bundle (`skill/SKILL.md`). `src/surface/card/mod.rs` is the stable card surface (`Budget`, `SymbolCard`, `FileCard`, `CardCompiler`, `Freshness`, `SourceStore`) with `git.rs` for Git projections, `types.rs` for card payload structs, `compiler/` for `GraphCardCompiler` (split into file.rs, io.rs, mod.rs, resolve.rs, symbol.rs), and `decision.rs` for `DecisionCard`. `src/surface/mcp/` holds the MCP tool handlers (helpers, cards, search, audit, findings, primitives) and `SynrepoState`; the server dispatch lives in `src/bin/cli_support/commands/mcp.rs`.
- Spec: `openspec/specs/cards/spec.md`, `openspec/specs/mcp-surface/spec.md`

**Bootstrap** (`src/bootstrap/`) — First-run UX, mode detection, health checks. `src/bin/cli.rs` is a thin dispatcher only; all logic lives here.
- `init/` — `bootstrap()` orchestrator: builds substrate index, runs structural compile, writes config and snapshot
- `report.rs` — `BootstrapReport`, `BootstrapHealth`, `BootstrapAction`
- `mode_inspect.rs` — auto vs curated mode detection via `inspect_repository_mode()`
- Spec: `openspec/specs/bootstrap/spec.md`

**Pipeline** (`src/pipeline/`) — `structural/` defines the 8-stage compile cycle. `mod.rs` owns transaction orchestration and `CompileSummary`; `stages.rs` owns stages 1–3 (discover → parse code → parse prose); `stage4.rs` owns cross-file edge resolution. Stage 5 (git mining) runs via `src/pipeline/git/` and `src/pipeline/git_intelligence/`. Stage 6 (identity cascade: content-hash, split, merge, git rename fallback, breakage) is wired. Stage 7 (drift scoring via Jaccard distance on persisted structural fingerprints) is wired and writes to the sidecar `edge_drift` and `file_fingerprints` tables. Stage 8 (ArcSwap commit) is not yet wired. `synthesis/` defines the `CommentaryGenerator` trait boundary with `stub.rs` (`NoOpGenerator`, default) and `claude.rs` (`ClaudeCommentaryGenerator`, reads `SYNREPO_ANTHROPIC_API_KEY`); called explicitly via the the `synrepo_refresh_commentary` tool or sync repair actions.
- `maintenance.rs` — storage-compatibility cleanup and compaction hooks; driven by `sync`.
- `repair/` — `mod.rs` is a thin façade. `report/` holds the drift-report builder with `surfaces/` (10 `SurfaceCheck` implementations split into `mod.rs`, `commentary.rs`, `cross_links.rs`, `drift.rs`). `sync.rs` drives auto-repair, `cross_links.rs` runs the cross-link generation pass, `log.rs` appends JSONL resolution records, `declared_links.rs` verifies `Governs` targets, `commentary.rs` is the commentary-refresh repair action that calls the synthesis generator, and `types/` holds the stable enums plus report/log payload types.
- `git_intelligence/` — `mod.rs` is a thin façade. `types.rs` defines the public Git-intelligence payloads, `analysis.rs` derives history/hotspot/ownership/co-change summaries, `emit.rs` emits `CoChangesWith` edges into the graph after each git pass, `symbol_revisions/` tracks per-symbol `first_seen_rev`/`last_modified_rev` via body-hash diffing, and `tests/` is split by status, history, path, and shared support helpers.
- `watch/` — reconcile backstop, watch lease/control plane, and watch loop production logic; tests live in `src/pipeline/watch/tests.rs`.
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
- `.synrepo/state/writer.lock` — process-level write lock (PID + timestamp); held during each actual runtime mutation, including watch-triggered reconcile passes
- `.synrepo/state/watch-daemon.json` — per-repo watch lease plus owner/telemetry snapshot for `synrepo watch`
- `.synrepo/state/watch-daemon.log` — stderr of the detached watch daemon; truncated on each spawn, useful for post-mortem on startup crashes
- `.synrepo/state/watch.sock` — local control socket for active daemon watch mode
- `.synrepo/state/reconcile-state.json` — last reconcile outcome, timestamp, and discovered/symbol counts
- `.synrepo/state/repair-log.jsonl` — append-only resolution log written by `synrepo sync`; one JSON object per line
- `openspec/` — planning artifacts only, not runtime

### Layer and size rules

- No layer may import from a layer above it. Substrate must not import from structure.
- Every `.rs` file must stay under 400 lines. Split into a sub-module directory before exceeding that limit.

### Workspace layout

`Cargo.toml` is a single-member workspace (the library + `synrepo` binary). The MCP server runs as a `synrepo mcp` subcommand — there is no separate binary crate. `rmcp`, `tokio`, and `schemars` are direct dependencies; `rmcp` and `tokio` are only used by the binary-side MCP command (`src/bin/cli_support/commands/mcp.rs`), keeping the library crate (`src/lib.rs`) synchronous.

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

Structural parsing supports Rust, Python, TypeScript/TSX, and Go.

Stages 1–3 run on every `synrepo init`:
1. **Discover** — substrate walk, `.gitignore`/`.synignore` respected
2. **Parse code** — tree-sitter symbol extraction; emits `FileNode`, `SymbolNode`, `Defines` edges (Rust, Python, TypeScript/TSX, Go)
3. **Parse prose** — concept node extraction from configured markdown directories

Stages 4–8:
4. Cross-file edge resolution (`calls`, `imports`, `inherits`, `references`) — **implemented** in `cards-and-mcp-v1`: name-based approximate resolution via tree-sitter call/import queries + post-parse name lookup pass in `src/pipeline/structural/stage4.rs`
5. Git mining (co-change, ownership, hotspots, recent file history) — **implemented** in `git-intelligence-v1`: deterministic first-parent history sampling via `src/pipeline/git/` and `src/pipeline/git_intelligence/`, surfaced today through file-facing outputs and node inspection
6. Identity cascade (rename detection) — **implemented**: content-hash rename, split/merge detection, git rename fallback all wired
7. Drift scoring — **implemented**: Jaccard distance on persisted structural fingerprints, sidecar `edge_drift` and `file_fingerprints` tables, all-edge enumeration
8. ArcSwap commit — TODO stub

### Overlay and audit surfaces

- Cross-link overlay store, card surfacing, CLI review flow, `synrepo_findings` MCP audit tool, and `synrepo sync --generate-cross-links` / `--regenerate-cross-links` are implemented in `cross-link-overlay-v1`.
- Cross-link promotion remains curated-mode-only. Accepted links become graph edges with `Epistemic::HumanDeclared`; overlay candidates stay in the audit trail.

### Shipped CLI surface (export-and-polish-v1)

- `synrepo export [--format markdown|json] [--deep] [--commit] [--out <dir>]` — generates `synrepo-context/` with rendered card output; added to `.gitignore` unless `--commit`.
- `synrepo upgrade [--apply]` — dry-run or apply storage compatibility actions; replaces the old "run `synrepo init`" recovery instruction for version-skew scenarios.
- `synrepo status [--json]` — enriched with export freshness and overlay cost summary.
- `synrepo agent-setup` — now accepts `codex` and `windsurf` targets, plus `--regen` flag for idempotent updates.

### Shipped risk assessment surface (change-risk-card-v1)

- `synrepo change-risk <target> [--budget tiny|normal|deep] [--json]` — computes change risk assessment for a file or symbol target, aggregating drift score, co-change partners, and git hotspot signals.
- `synrepo_change_risk` MCP tool — on-demand risk assessment via MCP protocol.

### Not yet implemented

- ArcSwap commit (stage 8) remains TODO.

## Gotchas

- **`src/bin/cli_support/agent_shims/` is a sub-module directory** — the canonical agent-doctrine text lives in `doctrine.rs` as a `doctrine_block!()` macro that every shim in `shims.rs` embeds via `concat!`. Edits to shim copy that touch escalation rules, do-not rules, or the product-boundary paragraph MUST go through `doctrine_block!`; the byte-identical test in `tests.rs` (`every_shim_embeds_doctrine_block`) enforces this. The escalation-line source-scan test reads `src/bin/cli_support/commands/mcp.rs` — do not move the MCP tool registration out of that file without updating the test path. Edit target-specific sections (tool list framing, CLI fallback examples, file paths) directly in `shims.rs`.
- **`src/structure/parse/extract/` is a sub-module directory** (`mod.rs` ~388 lines, `qualname.rs` ~59 lines) — do not add more code to `mod.rs` without splitting further. **Over-limit (split before adding):**
  - `src/store/overlay/cross_links.rs` (972) — split into cross_links/ submodule
  - `src/surface/card/compiler/neighborhood.rs` (688) — split into neighborhood/ submodule
  - `src/pipeline/repair/sync.rs` (661)
  - `src/pipeline/synthesis/cross_link/triage.rs` (651)
  - `src/bin/cli_support/tests/links.rs` (645) — test file, consider splitting
  - `src/pipeline/maintenance.rs` (622)
  - `src/store/overlay/tests.rs` (614) — test file
  - `src/store/sqlite/ops.rs` (590)
  - `src/pipeline/compact.rs` (574)
  - `src/structure/identity.rs` (517)
  - `src/pipeline/structural/stages.rs` (500)
  - `src/substrate/embedding/index.rs` (495)
  - `src/surface/card/types.rs` (476)
  - `src/pipeline/watch/lease.rs` (462)
  **Watchlist (approaching limit):** `src/surface/card/git.rs` (446), `src/substrate/embedding/model.rs` (434), `src/pipeline/diagnostics.rs` (430), `src/bin/cli_support/commands/status.rs` (430), `src/bin/cli_support/commands/links.rs` (418), `src/pipeline/recent_activity/mod.rs` (364), `src/bin/cli_support/cli_args.rs` (361), `src/surface/card/compiler/call_path.rs` (358), `src/structure/graph/store.rs` (349), `src/config.rs` (345)
- **`signature` and `doc_comment` are populated** by `src/structure/parse/extract/mod.rs` for Rust (`///` line comments, declaration up to `{`/`;`), Python (docstring, `def` line up to `:`), and TypeScript/TSX (JSDoc `/** */`, declaration up to `{`). These fields are safe to use in all three languages.
- **Stage 4 cross-file edges are now emitted**: `Calls` (file→symbol, approximate name resolution) and `Imports` (file→file, relative path resolution) edges are produced by `run_structural_compile`. `Inherits`, `References`, `Mentions` are not yet emitted. `CoChangesWith` is emitted by stage 5 via `git_intelligence/emit.rs`, not stage 4. `SplitFrom` and `MergedFrom` are emitted by stage 6 (identity cascade) for split/merge cases.
- **`all_edges()` excludes retired edges.** Use `all_edges()` for drift scoring and card compilation (both only care about active edges). If you need to include retired edges (e.g. for compaction enumeration), query the `edges` table directly without the `retired_at_rev IS NULL` filter.
- **`criterion` is present in `Cargo.toml`**, but the documented test workflow still centers on `proptest` and `insta`. Use `criterion` only for explicit benchmark work.
- **`.synrepo/graph/nodes.db`** is the actual SQLite file. Code that opens the graph store uses `SqliteGraphStore::open(&graph_dir)` where `graph_dir` is `.synrepo/graph/`; the `nodes.db` name is internal to `src/store/sqlite/mod.rs`.
- **Compatibility blocks on version mismatch**: if `.synrepo/` contains a graph store whose recorded format version is newer than the current binary understands, `synrepo init` and all graph commands will error. Run `synrepo upgrade` to see recovery steps; for a full reset, remove `.synrepo/` and run `synrepo init`.
- **Git history mining uses `gix`** (not `git2`). The history collector in `pipeline/git/mod.rs` disables rewrite tracking for performance. The rename fallback in `pipeline/git/renames.rs` enables it separately for the identity cascade step 4. Both use the `gix` crate; all gix usage is centralized under `src/pipeline/git/`.
- **`notify` and `notify-debouncer-full` are in `Cargo.toml`** and are used by the shipped watch runtime in `src/pipeline/watch/service.rs`. The service runs both `synrepo watch` foreground mode and `synrepo watch --daemon`, with `.synrepo/` self-event suppression and startup reconcile before steady-state watching.
- **`concept_directories` config defaults**: `docs/concepts`, `docs/adr`, `docs/decisions`. Adding a fourth directory (e.g. `architecture/decisions`) requires a config-sensitive compatibility check — changing this field triggers a graph advisory in the compat report.
- **File rename detection is implemented (full identity cascade).** Content-hash rename, symbol-set split/merge detection, and git rename fallback are all wired. When a file is moved, the cascade preserves the `FileNodeId` and records old paths in `path_history`. `SplitFrom` and `MergedFrom` edges are emitted for split/merge cases.
- **FileNodeId is stable across content edits.** Content-hash changes no longer delete and re-insert file nodes. Instead, `content_hash` is a version field updated in place, and `last_observed_rev` advances. Symbols and edges owned by the file are retired (marked `retired_at_rev`) if not re-emitted in the new compile, or re-activated if they re-appear. This enables drift scoring across observation windows.
- **Retired observations are soft-deleted until compaction.** Symbols and edges with `retired_at_rev` set remain in the store but are excluded from `active_edges()` and card compilation. Physical deletion only occurs via `compact_retired(older_than_rev)` which runs during `synrepo sync` and `synrepo upgrade --apply`. The `retain_retired_revisions` config (default 10) controls how many revisions of retired observations to keep before compaction.
- **Watch lease and writer lock are separate**: `.synrepo/state/watch-daemon.json` records long-lived watch ownership, while `.synrepo/state/writer.lock` still guards each actual write. `synrepo reconcile` delegates to the watch owner when watch is active; unsupported mutating commands fail fast until watch is stopped. Remove stale watch or writer artifacts only after confirming the recorded PID is dead (`kill -0 <pid>` returns non-zero). The canonical structural write path remains `run_reconcile_pass()` in `src/pipeline/watch/reconcile.rs`.
- **`repair/types/` has dual string mappings**: `RepairSurface`, `DriftClass`, `Severity`, and `RepairAction` each have `#[serde(rename_all = "snake_case")]` AND a manual `as_str()` in `src/pipeline/repair/types/stable.rs`. Adding a new variant requires updating both. `RepairSurface::ProposedLinksOverlay`, `RepairSurface::ExportSurface`, `RepairAction::RevalidateLinks`, and `RepairAction::RegenerateExports` follow the same rule. The stable-identifier tests in `src/pipeline/repair/types/tests.rs` catch `as_str()` divergence from literals but do not cross-check serde output.
- **Structural compile is a single atomic transaction (stages 1–4)**: `run_structural_compile` wraps all four stages in one `BEGIN`/`COMMIT`. Stage 4 reads uncommitted nodes from stages 1–3 via SQLite read-your-own-writes on the same connection. The `with_transaction` helper that existed in `structural/mod.rs` has been removed; do not re-add it.
- **Reader snapshots are re-entrant**: `SqliteGraphStore::begin_read_snapshot` and the overlay equivalent use a `Mutex<usize>` depth counter. Only the outermost begin issues `BEGIN DEFERRED`; only the outermost end issues `COMMIT`. This lets an MCP handler wrap a request while `GraphCardCompiler` also wraps each method internally without tripping SQLite's "transaction within a transaction" error. Writer-side `begin`/`commit`/`rollback` is a separate lane (`&mut self`) and must not interleave with a read snapshot on the same handle. Note: `BEGIN DEFERRED` only upgrades to a real read transaction on the first SELECT, so the snapshot epoch is pinned at the first read, not at begin.
- **Both SQLite stores set `busy_timeout = 5000`** (see `src/store/sqlite/schema.rs` and `src/store/overlay/schema.rs`) so transient WAL checkpoint contention waits up to 5 s rather than surfacing `SQLITE_BUSY`. This becomes load-bearing when readers hold snapshots across writer commits.
- **Binary crate test visibility**: In `src/bin/cli_support/tests/`, functions are accessible as `crate::<name>` only if imported at the binary root (`cli.rs`) via `use`. Modules declared `mod <name>` inside `commands/` are private — tests cannot reference them by full path; use the re-exported name instead.
- **`bootstrap()` signature**: `synrepo::bootstrap::bootstrap(repo_root: &Path, mode: Option<Mode>)` — two args only. Does not accept a pre-built `Config` or `synrepo_dir`; it derives both internally.
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
| `retain_retired_revisions` | `10` | Compile revisions to keep retired observations before compaction deletes them |

## Reference docs

- `docs/FOUNDATION.md` — full foundational design: architecture, trust model, data model, pipelines, cross-linking, operational requirements, evaluation
- `docs/FOUNDATION-SPEC.md` — product spec: card types, budget tiers, MCP tool surface, phased build plan, acceptance target

## OpenSpec workflow

`openspec/specs/` holds enduring domain specs (stable intended behavior). `openspec/changes/<name>/` holds active work: `proposal.md`, `design.md`, `tasks.md`, and optional delta specs.

Active changes: `graph-lifecycle-v1`, `structural-resilience-v2`, `semantic-triage-v1`, `storage-compaction-v1`

Archived completed changes are in `openspec/changes/archive/`.

Specs govern intent; the graph governs runtime truth.
