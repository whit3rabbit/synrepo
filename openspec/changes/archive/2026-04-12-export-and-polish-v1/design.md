## Context

Milestones 0–5 delivered the full structural core, optional-intelligence layers, and repair loop. The shipped surface is comprehensive but has three gaps for broader adoption:

1. **No offline export path**: agents without a live MCP session cannot read structured summaries; they fall back to raw file reads which defeats the point of synrepo.
2. **No upgrade command**: when the binary updates and `.synrepo/` contains older-version state, the user encounters cryptic compatibility errors with no guided recovery path.
3. **Narrow language coverage**: Go is widely used in the target audience but has no structural support; new users on Go repos get substrate-only value.

This change adds these missing pieces plus quality-of-life improvements to agent-setup and status output.

## Goals / Non-Goals

**Goals:**
- `synrepo export` command writing markdown or JSON convenience snapshots of card state
- Export freshness tracking and repair-loop participation via a new `ExportSurface`
- `synrepo upgrade` command with per-store compatibility action dispatch
- Go structural support (functions, types, interfaces, imports, calls)
- `synrepo agent-setup` target expansion (cursor, codex, windsurf) with `--regen` flag
- `synrepo status` additions: export freshness summary and overlay cost-to-date

**Non-Goals:**
- Exports as synthesis input or graph truth (invariant 2)
- Embedding exports in the graph store
- Real-time export updates (exports are produced on demand or via sync)
- Semantic search or embedding-based language support
- Automatic upgrade on startup (upgrade must be explicit)
- Grammar support for C, Java, or other languages (deferred to future changes)

## Decisions

### Decision 1: Export destination is `synrepo-context/` in the repo root, gitignored by default

**Why `synrepo-context/` over `.synrepo/exports/`**: The purpose of exports is to be readable by agents and contributors without navigating hidden directories. An explicit top-level directory makes it visible and clear. `.synrepo/` is the runtime store; mixing human-readable exports there violates the storage boundary.

**Gitignore behavior**: `synrepo init` and `synrepo export` both add `synrepo-context/` to `.gitignore` by default; a `--commit` flag allows explicit opt-in to tracking exports in source control. Repos that want to check in snapshots can do so intentionally.

**Alternative considered**: write to a user-specified path via `--out`. Rejected because it would make the repair-loop freshness tracking non-deterministic (we would not know where to look for stale exports without stored state). The path is configurable via `Config::export_dir` but defaults to `synrepo-context/`.

### Decision 2: Markdown default, JSON opt-in; one file per card type

**Format**: `synrepo export` produces one markdown file per compiled card type by default: `synrepo-context/symbols.md`, `synrepo-context/files.md`, `synrepo-context/decisions.md`. Each file is a structured summary at `Normal` budget (not `Deep`, to limit size). `--format json` produces a single `synrepo-context/index.json` manifest with the same content.

**Why `Normal` budget**: `Deep` exports including commentary and cross-links can be large and LLM-costly. The export is meant for offline orientation; if an agent needs deep detail on a specific node it should use the live MCP surface. A `--deep` flag overrides for cases where a fat snapshot is explicitly wanted.

**Alternative considered**: a single combined markdown document. Rejected because splitting by card type makes it easier for agents to load only the relevant section.

### Decision 3: Export freshness via an ExportManifest file

The export directory contains a `.export-manifest.json` tracking the graph schema version and last-reconcile timestamp at export time. `synrepo check` compares these against current runtime state; if the graph has been reconciled since the last export, the `ExportSurface` reports stale. This mirrors how commentary and cross-link freshness work (content-hash comparison) but at directory granularity rather than per-row.

**Why not per-file hashes**: per-file hashing adds complexity without much benefit — the cards are a derivative of the graph, and any graph reconcile could change any card. Tracking the reconcile epoch is sufficient for "is the export worth reading?" freshness.

### Decision 4: `synrepo upgrade` is explicit, not triggered on startup

**Why explicit only**: upgrade involves potentially destructive operations (rebuild, migrate). Running it silently on startup would violate the principle that data mutations require user intent. On startup, synrepo checks for version skew and emits a warning suggesting `synrepo upgrade`; it does not run the upgrade automatically.

**What upgrade does**: reads each `.synrepo/` store's recorded schema version, compares to the binary's declared supported range, applies the compatibility action from the existing `CompatibilityReport` infrastructure (which already has `continue`, `rebuild`, `invalidate`, `clear-and-recreate`, `migrate-required`, `block` actions), and emits a structured upgrade report. The upgrade command reuses the existing compatibility evaluator; it does not add a new migration layer.

### Decision 5: Go structural support uses the existing parse adapter pattern

Go parsing follows the same four-layer pattern as Rust/Python/TypeScript: tree-sitter grammar, adapter module in `src/structure/parse/`, symbol extraction producing `ExtractedSymbol` records, and call/import extraction for stage 4 edges. The existing `GoAdapter` lives in `src/structure/parse/go.rs`.

**Symbol kinds**: `Function`, `Method`, `TypeDef`, `Interface`, `Const`, `Var` (mapped to the existing `SymbolKind` enum, adding `TypeDef` and `Interface` variants if not already present).

**Why Go before others**: Go is the most commonly requested language among the target audience (AI coding agent users), has a mature tree-sitter grammar (`tree-sitter-go`), and has clean import/call graph semantics that match the existing stage 4 edge model.

### Decision 6: agent-setup target expansion uses a template-per-tool model

Each `agent-setup` target (claude, cursor, copilot, generic, codex, windsurf) maps to a template in `src/bootstrap/shim/`. Templates are static strings (not runtime-rendered from the graph). The `--regen` flag compares the current output file against the template and overwrites if different, printing a diff summary.

### Decision 7: No new open questions — the three design.md open questions from cross-link-overlay-v1 are resolved context here

This change does not inherit unresolved questions from prior changes. All decisions above are self-contained.

## Risks / Trade-offs

**Go grammar version pinning**: `tree-sitter-go` grammars have had breaking query changes in the past. Risk: query behavior drifts between grammar upgrades. Mitigation: pin the grammar version in `Cargo.toml` and add a grammar validation test that confirms expected symbol counts on a known Go fixture.

**Export directory confusion**: users may expect `synrepo-context/` to be authoritative documentation and edit it. Risk: edited exports are overwritten silently by next sync. Mitigation: add a prominent generated-file header to each export file; document in bootstrap that the directory is generated output.

**Upgrade command destructive operations**: `rebuild` action deletes and recreates a store. Risk: user runs upgrade expecting a soft migration and loses cached data. Mitigation: dry-run by default (`--apply` required to execute mutations); print a plan before executing.

**Status enrichment cost**: reading overlay cost-to-date requires querying the overlay audit tables. Risk: adds latency to `synrepo status` on repos with large audit tables. Mitigation: read counts only (no full row scans); cache the result in `reconcile-state.json`.

## Migration Plan

No breaking storage changes. All new CLI commands are additive. Go adapter adds a new grammar dependency; users without Go repos are unaffected. `synrepo-context/` is created only when `synrepo export` is run. Upgrade command is explicit-only; no startup behavior changes.

## Open Questions

None. All design decisions above are settled based on existing implementation patterns and roadmap constraints.
