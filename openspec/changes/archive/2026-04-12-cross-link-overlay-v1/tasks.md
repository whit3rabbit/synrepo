## 1. Storage layer — `cross_links` + `cross_link_audit`

- [x] 1.1 Add `cross_links` and `cross_link_audit` table DDL to `src/store/overlay/schema.rs`; bump overlay store version from `v1` to `v2` with a migration that runs `CREATE TABLE IF NOT EXISTS` only
- [x] 1.2 Create `src/store/overlay/cross_links.rs` with `insert_candidate`, `candidate_for_pair`, `candidates_for_node`, `update_tier`, `mark_rejected`, `mark_promoted`, `prune_orphans` helpers
- [x] 1.3 Create `src/store/overlay/cross_link_audit.rs` with `append_event` and `events_for_candidate` helpers; enforce immutability via SQL (no UPDATE/DELETE grants through the accessor)
- [x] 1.4 Wire `SqliteOverlayStore::insert_link` and `links_for` to the new table; drop the stub behavior
- [x] 1.5 Extend `prune_orphans` on `SqliteOverlayStore` to cover cross-links too; append `cross_link_audit` rows for every pruned candidate with reason `source_deleted`
- [x] 1.6 Extend `src/store/overlay/tests.rs` with coverage for insert, retrieve-by-node, prune-orphans audit trail, and write-isolation negative tests (commentary writer cannot write cross-link rows)

## 2. Types and provenance

- [x] 2.1 Add `CrossLinkProvenance` struct (pass_id, model_identity, generated_at) in `src/overlay/mod.rs`; require it on `OverlayLink`
- [x] 2.2 Extend `OverlayLink` with `from_content_hash`, `to_content_hash`, `confidence_score` (f32), `confidence_tier` (enum)
- [x] 2.3 Add `ConfidenceTier` enum (`High`, `ReviewQueue`, `BelowThreshold`) with `as_str()` and `#[serde(rename_all = "snake_case")]`
- [x] 2.4 Add `CrossLinkFreshness` enum (`Fresh`, `Stale`, `SourceDeleted`, `Invalid`, `Missing`) with `as_str()` and serde snake_case
- [x] 2.5 Unit tests in `src/overlay/mod.rs` confirming freshness derivation matches the spec five-state model

## 3. Candidate generation — triage and scoring

- [x] 3.1 Create `src/pipeline/synthesis/cross_link.rs` with `CrossLinkGenerator` trait `generate_candidates(&self, scope: &CandidateScope) -> Result<Vec<OverlayLink>>`
- [x] 3.2 Implement `NoOpCrossLinkGenerator` returning `Ok(vec![])` in `src/pipeline/synthesis/cross_link.rs`
- [x] 3.3 Implement the deterministic prefilter in a separate `triage.rs` submodule: `candidate_pairs(graph, scope) -> Vec<(NodeId, NodeId, OverlayEdgeKind)>` using name-match and graph-distance cutoff
- [x] 3.4 Implement `ClaudeCrossLinkGenerator` in `src/pipeline/synthesis/cross_link_claude.rs`; reuse the `Claude` client from `claude.rs` for HTTP and keying
- [x] 3.5 Implement the confidence scoring function `score(spans: &[CitedSpan], graph_distance: u32) -> (f32, ConfidenceTier)` with thresholds from config
- [x] 3.6 Tests: triage prefilter returns zero pairs for names that don't match and distance > cutoff; scoring function produces `high` only when all specified criteria hold

## 4. Config

- [x] 4.1 Add `cross_link_cost_limit: u32` (default 200) and `cross_link_confidence_thresholds: { high: f32, review_queue: f32 }` fields to `Config` in `src/config.rs`
- [x] 4.2 Mark the threshold fields as compatibility-advisory (not rebuild-triggering) in the compat evaluator
- [x] 4.3 Update `.synrepo/config.toml` defaults writer and the `Config` doc comments
- [x] 4.4 Test: round-trip `Config` serialization preserves new fields; changing a threshold produces a compat advisory, not a rebuild

## 5. Card compiler wiring

- [x] 5.1 Add `ProposedLink` payload struct in `src/surface/card/types.rs` (endpoint IDs, kind, tier, freshness, span count)
- [x] 5.2 Add `proposed_links: Option<Vec<ProposedLink>>` and `links_state: Option<String>` fields to `SymbolCard` and `FileCard` in `types.rs`
- [x] 5.3 In `src/surface/card/compiler/symbol.rs` and `compiler/file.rs`, at `Deep` budget query `links_for(node)` and populate `proposed_links`; at `Tiny`/`Normal` set `links_state: "budget_withheld"`
- [x] 5.4 Filter out `below_threshold` candidates in the compiler before populating `proposed_links`
- [x] 5.5 Compute per-candidate freshness at compile time from stored hashes vs current `FileNode.content_hash`; set the `freshness` field on each `ProposedLink`
- [x] 5.6 Add insta snapshot tests covering: fresh-high-tier link on `SymbolCard`, stale link preservation, budget-withheld at `Normal`, missing state when no candidates exist, `below_threshold` filtered out at `Deep`

## 6. Repair loop — `proposed_links_overlay` surface

- [x] 6.1 Add `RepairSurface::ProposedLinksOverlay` variant in `src/pipeline/repair/types/stable.rs`; update both serde snake_case and `as_str()`
- [x] 6.2 Add `RepairAction::RevalidateLinks` variant in the same file; update both mappings
- [x] 6.3 Add `DriftClass::SourceDeleted` variant if not present; update mappings
- [x] 6.4 Extend `src/pipeline/repair/report.rs` to produce findings for the new surface: classify each `cross_links` row as `fresh`/`stale`/`source_deleted` using both endpoint hashes; emit one finding per drifted row
- [x] 6.5 Extend `src/pipeline/repair/sync.rs` with `revalidate_links` action: re-run fuzzy-LCS verifier against stored `CitedSpan`s using current source text; refresh hashes on success, demote tier on failure, append audit row either way _(PR 1 wires dispatch-path only; verifier ships in PR 2 alongside source-text loading)_
- [x] 6.6 Tests: check reports stale/source_deleted surfaces correctly; sync refreshes candidates whose spans still verify; sync demotes candidates whose spans no longer verify; stable-identifier tests updated for the new variants _(check/stable-identifier coverage landed in PR 1; sync verifier coverage deferred to PR 2)_

## 7. CLI — `synrepo links` and `synrepo findings`

- [x] 7.1 Add `synrepo links list [--tier <tier>] [--json]` subcommand in `src/bin/cli_support/commands.rs`; routes through `SqliteOverlayStore::candidates_for_node` for all stored candidates
- [x] 7.2 Add `synrepo links review [--limit <n>] [--json]` — returns `review_queue`-tier candidates only, sorted by confidence score descending
- [x] 7.3 Add `synrepo links accept <candidate-id> [--reviewer <name>]` — curated mode only; writes graph edge with `Epistemic::HumanDeclared` + reviewer identity, updates candidate with `promoted_at` + `graph_edge_id`, appends audit row. Errors cleanly in auto mode.
- [x] 7.4 Add `synrepo links reject <candidate-id> [--reviewer <name>]` — updates candidate to `rejected` state, appends audit row
- [x] 7.5 Add `synrepo findings [--node <id>] [--kind <kind>] [--freshness <state>] [--json]` — returns `review_queue` + `below_threshold` + `source_deleted` candidates with full provenance; no tier filter by default
- [x] 7.6 Tests in `src/bin/cli_support/tests/`: links accept is blocked in auto mode; accept writes a `HumanDeclared` edge; reject updates state; findings returns below-threshold candidates

## 8. MCP — `synrepo_findings` tool

- [x] 8.1 Add `synrepo_findings` tool handler in `crates/synrepo-mcp/src/main.rs` with params `node_id?`, `kind?`, `freshness?`, `limit?`
- [x] 8.2 Implement the handler by calling the same overlay-store path as the CLI `findings` command
- [x] 8.3 Ensure the response schema matches the spec: provenance, tier, score, freshness, endpoint IDs
- [x] 8.4 Update the MCP server's tool description string; update the agent orientation text that lists available tools

## 9. Candidate generation pass — hook into CLI

- [x] 9.1 Add `synrepo sync --generate-cross-links` flag in `commands.rs`; runs the full pipeline: triage prefilter → LLM generation (`ClaudeCrossLinkGenerator` or `NoOp`) → evidence verification → confidence scoring → persistence
- [x] 9.2 Respect `cross_link_cost_limit`: bail with a report-only summary once the limit is hit; report remaining candidates as `blocked`
- [x] 9.3 Add `synrepo sync --regenerate-cross-links` that re-runs full generation for stale candidates (in contrast to `revalidate_links`, which is deterministic-verification-only)
- [x] 9.4 Tests: run generation against a small fixture repo with a fake `CrossLinkGenerator`; verify stored rows, audit trail, and tier distribution

## 10. Reconcile integration

- [x] 10.1 Extend `run_reconcile_pass` in `src/pipeline/watch.rs` to call the extended `prune_orphans` after the structural compile completes
- [x] 10.2 Tests in `src/pipeline/watch/tests.rs`: a file deletion removes orphan cross-links on the next reconcile, and audit rows are written

## 11. Validation and documentation

- [x] 11.1 Run `openspec validate cross-link-overlay-v1` and confirm it passes
- [x] 11.2 Run `make check` (fmt-check + clippy + test); resolve any lint or test failures
- [x] 11.3 Update `CLAUDE.md` Gotchas section: note `RepairSurface::ProposedLinksOverlay` and `RepairAction::RevalidateLinks` require dual-mapping updates (matches existing commentary gotcha)
- [x] 11.4 Update `CLAUDE.md` Phase status section: mark cross-link overlay implemented; update shipped/not-shipped MCP tool list
- [x] 11.5 Update `AGENTS.md` Active changes section to reflect the new active change while work is in progress (and back to "none" after archive)
- [x] 11.6 After implementation is complete and tests pass, run `/opsx:archive` to archive the change and sync delta specs into the main specs tree
