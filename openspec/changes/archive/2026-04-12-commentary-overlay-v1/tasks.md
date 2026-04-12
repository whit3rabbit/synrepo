## 1. Commentary types in overlay module

- [x] 1.1 Add `CommentaryProvenance` struct to `src/overlay/mod.rs`: fields `source_content_hash: String`, `pass_id: String`, `model_identity: String`, `generated_at: time::OffsetDateTime`
- [x] 1.2 Add `CommentaryEntry` struct to `src/overlay/mod.rs`: fields `node_id: NodeId`, `text: String`, `provenance: CommentaryProvenance`
- [x] 1.3 Add `FreshnessState` enum to `src/overlay/mod.rs` with variants `Fresh`, `Stale`, `Invalid`, `Missing`, `Unsupported`; add `#[serde(rename_all = "snake_case")]`
- [x] 1.4 Add commentary methods to the `OverlayStore` trait: `insert_commentary(&mut self, entry: CommentaryEntry) -> Result<()>`, `commentary_for(&self, node: NodeId) -> Result<Option<CommentaryEntry>>`, `prune_orphans(&mut self, live_nodes: &[NodeId]) -> Result<usize>`
- [x] 1.5 Confirm `src/overlay/mod.rs` compiles with no warnings (`cargo build 2>&1 | grep "^error"` returns nothing)

## 2. SQLite overlay store

- [x] 2.1 Create `src/store/overlay/` directory with `mod.rs`, `schema.rs`, `commentary.rs`; add `pub mod overlay;` to `src/store/mod.rs`
- [x] 2.2 Implement `schema.rs`: `init_schema` creates `.synrepo/overlay/overlay.db` with a `commentary` table (`id INTEGER PRIMARY KEY, node_id TEXT NOT NULL UNIQUE, text TEXT NOT NULL, source_content_hash TEXT NOT NULL, pass_id TEXT NOT NULL, model_identity TEXT NOT NULL, generated_at TEXT NOT NULL`)
- [x] 2.3 Implement `SqliteOverlayStore` in `mod.rs`: `open(overlay_dir: &Path) -> Result<Self>`, opens/creates `overlay.db`, calls `init_schema`
- [x] 2.4 Implement `OverlayStore` commentary methods on `SqliteOverlayStore` in `commentary.rs`: `insert_commentary` upserts by `node_id`; `commentary_for` returns `None` on miss, validates all provenance fields (returns `FreshnessState::Invalid` on missing); `prune_orphans` deletes rows not in `live_nodes`
- [x] 2.5 Add `src/store/overlay/tests/` with integration tests: round-trip insert/retrieve, orphan pruning, provenance validation rejection, schema isolation (no graph tables in overlay db)
- [x] 2.6 Confirm `cargo test` passes for the new store tests

## 3. Freshness derivation

- [x] 3.1 Add `derive_freshness(entry: &CommentaryEntry, current_content_hash: &str) -> FreshnessState` free function in `src/store/overlay/commentary.rs`: returns `Fresh` on hash match, `Stale` on mismatch, `Invalid` if provenance fields are empty
- [x] 3.2 Add unit tests for `derive_freshness` covering all three states
- [x] 3.3 Confirm `Freshness` in `src/surface/card/types.rs` is extended with `Invalid` and `Unsupported` variants (matching `FreshnessState`); update snapshot tests if any break

## 4. Synthesis pipeline and commentary generator

- [x] 4.1 Convert `src/pipeline/synthesis.rs` to `src/pipeline/synthesis/mod.rs`; add `stub.rs`; add `pub mod synthesis;` remains in `src/pipeline/mod.rs`
- [x] 4.2 Define `CommentaryGenerator` trait in `mod.rs`: `fn generate(&self, node: NodeId, context: &str) -> Result<Option<CommentaryEntry>>`; context is the symbol card text passed as input
- [x] 4.3 Implement `NoOpGenerator` in `stub.rs`: always returns `Ok(None)`
- [x] 4.4 Add `reqwest` to `Cargo.toml` dependencies: `reqwest = { version = "0.12", default-features = false, features = ["rustls-tls", "json"] }`
- [x] 4.5 Create `src/pipeline/synthesis/claude.rs`: implement `ClaudeCommentaryGenerator` calling Claude Messages API; reads `SYNREPO_ANTHROPIC_API_KEY` from env; falls back to `NoOpGenerator` if key is absent; respects `commentary_cost_limit` from config
- [x] 4.6 Add `commentary_cost_limit: u32` to `src/config.rs` with default `5000` (approximate token budget per generation call); add to config table in `CLAUDE.md`
- [x] 4.7 Confirm `cargo build` succeeds with `reqwest` added; run `cargo clippy -- -D warnings`

## 5. Card compiler overlay integration

- [x] 5.1 Add `overlay: Option<Arc<dyn OverlayStore>>` and `generator: Option<Arc<dyn CommentaryGenerator>>` fields to `GraphCardCompiler`; add constructor `with_overlay` that accepts both
- [x] 5.2 In `src/surface/card/compiler/symbol.rs`: after building the structural `SymbolCard`, if budget is `Deep` and overlay store is present, call `commentary_for(id)`; if `None` and generator is present, call `generate(id, card_text)` and store result; populate `overlay_commentary` with the entry text and derived freshness
- [x] 5.3 Add `commentary_state: Option<String>` field to `SymbolCard` in `src/surface/card/types.rs`; set to `"budget_withheld"` at `Tiny`/`Normal` budget, or the freshness state string at `Deep` budget
- [x] 5.4 Update insta snapshot tests: add new snapshots for `SymbolCard` at `Deep` budget with mock fresh commentary, stale commentary, and no commentary (missing state)
- [x] 5.5 Confirm `cargo test` passes for card compiler tests

## 6. MCP surface: commentary state in synrepo_card

- [x] 6.1 In `crates/synrepo-mcp/`, update the `synrepo_card` tool handler to pass the overlay store and generator to the card compiler; thread `SqliteOverlayStore` open from the MCP server startup
- [x] 6.2 Ensure the `synrepo_card` JSON response includes `commentary_state` and, when non-null, `commentary_text` fields
- [x] 6.3 Confirm `cargo build` for the MCP crate succeeds

## 7. Repair loop: commentary overlay surface

- [x] 7.1 In `src/pipeline/repair/types/stable.rs`: rename `RepairSurface::OverlayEntries` to `CommentaryOverlayEntries`; update `as_str()` to return `"commentary_overlay_entries"`; update `RepairAction` with new `RefreshCommentary` variant and `as_str()` returning `"refresh_commentary"`
- [x] 7.2 Update both `#[serde]` and `as_str()` for the renamed/added variants; update stable-identifier tests in `src/pipeline/repair/types/tests.rs` to cover the new strings
- [x] 7.3 In `src/pipeline/repair/report.rs`: change the `OverlayEntries` surface check from always-`Unsupported` to: absent (no `overlay.db`) → `DriftClass::Absent`; present with stale entries → `DriftClass::Stale` + `RepairAction::RefreshCommentary`; present and current → `DriftClass::Current`
- [x] 7.4 In `src/pipeline/repair/sync.rs`: add handler for `RepairAction::RefreshCommentary` that opens `SqliteOverlayStore`, fetches stale entries, calls `CommentaryGenerator::generate` for each within budget, persists results, logs outcomes
- [x] 7.5 Confirm `cargo test` for repair tests passes; check both `check` and `sync` path tests cover the new surface

## 8. Reconcile: orphan pruning

- [x] 8.1 In the reconcile path (`src/pipeline/watch.rs` or `run_reconcile_pass`): after the structural compile completes, open `SqliteOverlayStore` if `overlay.db` exists and call `prune_orphans` with the current node ID list from the graph
- [x] 8.2 Log the count of pruned entries at `debug` level
- [x] 8.3 Confirm no regression in existing reconcile/watch tests

## 9. Status command: commentary coverage

- [x] 9.1 In the `synrepo status` output, add a commentary coverage line: `commentary: N fresh / M total nodes with commentary` (or "commentary: not initialized" if `overlay.db` absent)
- [x] 9.2 Confirm `cargo run -- status` prints the new line without panicking on a repo with no overlay db

## 10. Durable spec updates

- [x] 10.1 Apply delta specs to `openspec/specs/cards/spec.md`: replace the "Distinguish graph-backed and overlay-backed card fields" requirement with the modified version from `specs/cards/spec.md`
- [x] 10.2 Apply delta specs to `openspec/specs/repair-loop/spec.md`: add the two new requirements from `specs/repair-loop/spec.md`
- [x] 10.3 Create `openspec/specs/commentary-store/spec.md` by copying `specs/commentary-store/spec.md`
- [x] 10.4 Update `ROADMAP.md` section 0 "Active change" line to reflect `commentary-overlay-v1` in progress

## 11. Validation

- [x] 11.1 Run `cargo test` — all tests pass
- [x] 11.2 Run `cargo clippy -- -D warnings` — no warnings
- [x] 11.3 Run `cargo run -- init` on a test repo, then `cargo run -- node <id>` for a symbol node — confirm no panics
- [x] 11.4 Run `cargo run -- check` — confirm `commentary_overlay_entries` appears as `absent` (not `unsupported`) before any overlay.db exists
- [x] 11.5 Confirm `openspec/specs/overlay/overlay.db` path is NOT created by `synrepo init` (overlay db is lazy, created on first commentary request only)
- [x] 11.6 Confirm `openspec/specs/graph/nodes.db` has no `commentary` table (schema isolation)
