## Context

The overlay module boundary (`src/overlay/mod.rs`) is established and carries `OverlayStore` trait, `OverlayLink`, and `OverlayEpistemic` types, but has no SQLite backend. `src/pipeline/synthesis.rs` is a 4-line stub. Cards hardcode `overlay_commentary: None`. The `Freshness` enum in `src/surface/card/types.rs` has three variants (`Fresh`, `Stale`, `Missing`) against the spec's five states (`fresh`, `stale`, `invalid`, `missing`, `unsupported`). The repair loop reports `OverlayEntries` as `Unsupported`.

This change builds the complete commentary storage and retrieval path. Generation is included via a `CommentaryGenerator` trait with a no-op stub and a Claude-backed implementation.

## Goals / Non-Goals

**Goals:**

- Implement `src/store/overlay/` as a SQLite-backed commentary store at `.synrepo/overlay/overlay.db`.
- Define `CommentaryEntry` and `CommentaryProvenance` types in `src/overlay/mod.rs`.
- Align `Freshness` with all five spec states.
- Wire `GraphCardCompiler` to populate `overlay_commentary` at `Deep` budget from the store.
- Add `CommentaryGenerator` trait in `src/pipeline/synthesis/` with a no-op stub and a Claude API implementation.
- Return explicit `commentary_state` label in `synrepo_card` MCP responses.
- Activate `OverlayEntries` as a live repair surface (stale-commentary detection and refresh in `synrepo check` / `synrepo sync`).
- Add `overlay_commentary_coverage` to `synrepo status` output.

**Non-Goals:**

- Cross-link overlay (`OverlayLink`, `OverlayEdgeKind`) — those types remain as stubs.
- Eager pre-generation during `synrepo init` or `synrepo reconcile`.
- Commentary on `FileCard` or `DecisionCard` — `SymbolCard` only in this slice.
- Multiple LLM providers — Claude API only; provider abstraction is future work.
- Body-hash normalization (whitespace/rename-insensitive freshness) — out of scope for v1.

## Decisions

### Decision: Separate `SqliteOverlayStore` from the graph store

`src/store/overlay/` follows the same pattern as `src/store/sqlite/`: `mod.rs` opens/creates the database, `schema.rs` owns `init_schema`, `commentary.rs` owns the commentary CRUD methods.

Alternative: add a `commentary` table to `nodes.db`. Rejected: the hard architectural invariant is that the overlay lives in a physically separate SQLite database from the graph. Co-locating them would violate invariant 2 and make it easier for future code to accidentally join overlay and graph data in a single query.

### Decision: `OverlayStore` trait gains commentary methods; `OverlayLink` methods remain as stubs

Rather than a separate `CommentaryStore` trait, the commentary methods are added to `OverlayStore` alongside the existing link methods. This keeps a single trait boundary for the overlay physical store.

Alternative: a new `CommentaryStore` trait. Rejected: the overlay is one physical store. Adding a second trait creates two separate "overlay" concepts at the trait level, which contradicts the single-store boundary. A single trait with clearly grouped commentary vs link methods is cleaner.

### Decision: Freshness derivation by content-hash comparison

A `CommentaryEntry` stores `source_content_hash: String` — the `FileNode.content_hash` of the file at generation time. Freshness is `Fresh` if the stored hash matches the current `FileNode.content_hash`; `Stale` if it differs; `Invalid` if required provenance fields are absent; `Unsupported` if the node kind has no commentary pipeline.

Alternative: use a graph revision counter or timestamp comparison. Rejected: content hashes are already stable identities in synrepo and are cheaper to compare than maintaining a global revision sequence. A file that has not changed always yields `Fresh` without needing to track revision numbers.

### Decision: `CommentaryGenerator` trait in `src/pipeline/synthesis/`

`src/pipeline/synthesis.rs` becomes `src/pipeline/synthesis/` with `mod.rs` (trait), `stub.rs` (no-op), and `claude.rs` (Claude API). The live implementation calls the Claude Messages API with `SYNREPO_ANTHROPIC_API_KEY` from the environment. If the key is absent, the generator falls back to the stub silently.

Alternative: gate the Claude implementation behind a Cargo feature flag. Not chosen for now: a feature flag adds build complexity for what is effectively a runtime configuration. An absent key is the natural off switch.

New dependency: `reqwest` with `rustls-tls` features. This is the first HTTP client dep. It is required for the live generation path and is not optional at the crate level.

### Decision: Commentary generated lazily at `Deep` budget request time

When `GraphCardCompiler::symbol_card` is called at `Deep` budget and no overlay entry exists, it calls `CommentaryGenerator::generate`. If generation succeeds the result is stored and returned. If the generator is the stub or generation fails, `overlay_commentary` carries `Freshness::Missing`.

Alternative: generate on a background thread / async worker. Not for v1: the sync compiler and CLI are single-threaded; introducing async here requires tokio in the library crate. The MCP server is already async (in `crates/synrepo-mcp/`); commentary generation there can be non-blocking in a follow-on slice.

### Decision: `commentary_state` label is a flat field on the MCP `synrepo_card` response

The four observable states (present-fresh, present-stale, absent, budget-withheld) map to a JSON string field `commentary_state` on the response object. Commentary text is in `commentary_text` when state is `present_fresh` or `present_stale`.

Alternative: embed the state inside the `overlay_commentary` object. Rejected: callers should be able to check the state without deserializing nested fields. Parallel fields are more explicit for MCP consumers.

### Decision: `RepairAction::RefreshCommentary` — new variant for overlay refresh

The repair loop needs a new action that is distinct from `RunReconcile` (which refreshes the graph). A new `RepairAction::RefreshCommentary` variant signals that the commentary generation pass should be triggered for the affected node. The stable string is `"refresh_commentary"`. Both `as_str()` and serde must be updated per the CLAUDE.md gotcha.

### Decision: `RepairSurface::OverlayEntries` is renamed to `CommentaryOverlayEntries` for clarity

The current name `OverlayEntries` is ambiguous once cross-links also exist. Renaming to `CommentaryOverlayEntries` now (with `as_str()` → `"commentary_overlay_entries"`) is cheaper before any downstream tooling is built around the string identifier. The `repair/types/tests.rs` stable-identifier tests will catch any `as_str()` divergence.

## Risks / Trade-offs

- **Risk: reqwest increases binary size significantly** → Mitigation: `rustls-tls` feature avoids linking OpenSSL. Acceptable for v1; feature-flagging can be revisited if binary size becomes a documented concern.
- **Risk: Lazy generation at `Deep` budget adds latency to card requests** → Mitigation: generation is bounded by the configured `commentary_cost_limit` in config; if generation would exceed budget it is skipped and the state is `Missing`. A warning is logged. Cache hit (entry already in the overlay) avoids any generation cost.
- **Risk: Freshness check requires a live `GraphStore` read per card** → Mitigation: `FileNode.content_hash` is a single indexed row read; negligible overhead.
- **Risk: Claude API call fails mid-request** → Mitigation: the `CommentaryGenerator::generate` contract returns `Result<Option<CommentaryEntry>>`. `None` means "generation not attempted or failed gracefully." The card compiler treats this as `Freshness::Missing` and continues without commentary. The error is logged at `warn` level; the structural card is returned unchanged.
- **Risk: Commentary for a deleted symbol is not cleaned up** → Mitigation: add a garbage-collect pass in `SqliteOverlayStore::prune_orphans` that deletes commentary entries whose `node_id` no longer exists in the graph. Called during `synrepo reconcile`.
- **Risk: `RepairSurface` rename breaks serialized repair logs** → Mitigation: The rename from `overlay_entries` → `commentary_overlay_entries` only affects new outputs. Old repair-log entries with the old string are not re-parsed in normal operation. Add a note in the migration section.

## Migration Plan

1. `SqliteOverlayStore::open` creates `.synrepo/overlay/overlay.db` on first use. No action needed by existing users; the file is absent and is created automatically.
2. The `RepairSurface::CommentaryOverlayEntries` rename changes the serialized string. Existing repair logs contain the old `"overlay_entries"` string. No migration is needed: repair logs are append-only and are not re-read for decisions. The change is forward-only.
3. `Freshness::Invalid` and `Freshness::Unsupported` are new enum variants. Existing snapshot tests include `"overlay_commentary": null` — no snapshot update needed. New snapshot tests covering non-null commentary states are added in this change.

## Open Questions

None. All decisions are resolved above.
