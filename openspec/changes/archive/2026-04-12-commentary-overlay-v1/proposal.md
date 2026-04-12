## Why

The structural product (Milestone 3–4) is complete and the overlay spec contracts are defined. The `src/overlay/mod.rs` module boundary exists but has no SQLite backend, no commentary storage, and no generation path. Cards currently hardcode `overlay_commentary: None`. This change ships the first real overlay slice: persisted commentary with provenance, freshness tracking, lazy generation, and card/MCP surface integration.

## What Changes

- Add `src/store/overlay/` SQLite backend implementing commentary persistence: `commentary` table with provenance columns (source revision, pass ID, model identity, timestamp), freshness derivation, and a `CommentaryStore` trait.
- Extend `src/overlay/mod.rs` with commentary-specific types: `CommentaryEntry`, `FreshnessState` (fresh, stale, invalid, missing, unsupported), and `CommentaryProvenance`.
- Wire `GraphCardCompiler` to accept an optional `OverlayStore` reference and populate `SymbolCard.overlay_commentary` at `Deep` budget when a fresh or stale entry exists.
- Add a `CommentaryGenerator` trait in `src/pipeline/synthesis/` (thin, LLM-provider-agnostic) with a no-op stub and a Claude-backed implementation; lazy generation fires on first `Deep`-budget card request when no entry exists.
- Expose commentary state in MCP tool responses: `synrepo_card` returns commentary with explicit state label (fresh, stale, missing, budget-withheld).
- Add stale-commentary-overlay as an active repair surface in `synrepo check` / `synrepo sync` (currently reported as unsupported).
- Extend `synrepo status` to report commentary coverage (nodes with fresh entries vs total).

## Capabilities

### New Capabilities

- `commentary-store`: SQLite-backed overlay store for commentary entries with full provenance fields, freshness derivation, and lazy-generation lifecycle.

### Modified Capabilities

- `overlay`: Moving from a trait-only boundary to an implemented store. `OverlayStore` gets a concrete `SqliteOverlayStore` backed by `.synrepo/overlay/overlay.db`. Commentary types (`CommentaryEntry`, `CommentaryProvenance`, `FreshnessState`) replace the phase-0 stub variants.
- `cards`: `SymbolCard.overlay_commentary` is populated at `Deep` budget. `Freshness` enum gains `Invalid` and `Unsupported` variants to match the spec states.
- `mcp-surface`: `synrepo_card` response includes `commentary_state` label for all four observable states (present-fresh, present-stale, absent/missing, budget-withheld). No new MCP tools are added.
- `repair-loop`: Commentary staleness becomes an active repair surface (`DriftClass::StaleCommentaryOverlay`). `synrepo check` detects and reports stale entries; `synrepo sync` triggers refresh for auto-repairable stale entries within budget.

## Impact

- `src/overlay/mod.rs` — extended with commentary types; `OverlayStore` trait gains commentary methods alongside existing link methods
- `src/store/overlay/` — new sub-module: `mod.rs`, `schema.rs`, `commentary.rs`, `tests/`
- `src/pipeline/synthesis/` — new sub-module: `mod.rs` (trait), `stub.rs` (no-op), `claude.rs` (Claude API implementation)
- `src/surface/card/types.rs` — `Freshness` gains `Invalid` and `Unsupported` variants; `OverlayCommentary` gains `provenance` field
- `src/surface/card/compiler/` — `GraphCardCompiler` accepts optional overlay store; `symbol.rs` populates commentary at `Deep` budget
- `crates/synrepo-mcp/` — `synrepo_card` tool response includes `commentary_state`
- `src/pipeline/repair/` — `DriftClass::StaleCommentaryOverlay` activated; `sync.rs` gains commentary refresh path
- `src/bin/cli.rs` / status command — coverage metrics added
- `Cargo.toml` — no new deps required for stub path; Claude API client (already `anthropic` crate or `reqwest`) needed for the live path
- Storage: adds `.synrepo/overlay/overlay.db`; no changes to `.synrepo/graph/nodes.db`
