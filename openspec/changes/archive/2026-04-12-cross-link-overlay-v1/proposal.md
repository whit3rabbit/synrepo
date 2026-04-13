## Why

The graph carries only parser-observed, git-observed, and human-declared facts. Semantic cross-cutting relationships — "`docs/auth.md` discusses `fn authenticate`", "concept A is derived from concept B", "this ADR governs those three symbols" — are not observable by parsers, yet agents routinely need them to orient across code and prose. Track J (`commentary-overlay-v1`) proved the overlay pattern end-to-end; Track K (cross-links) is the next slice: bounded, auditable, evidence-verified link candidates that enrich cards without contaminating the graph.

The implementation scaffolding already exists (`OverlayLink`, `OverlayEdgeKind`, `CitedSpan`, `OverlayEpistemic`, stubbed `insert_link`/`links_for`), but the pipeline that produces and verifies candidates, the confidence model, the review/promotion workflow, and the drift-repair surface are not wired. This change wires them and closes Milestone 5's optional-intelligence layer.

## What Changes

- Add candidate cross-link generation in `src/pipeline/synthesis/`: an LLM-backed `CrossLinkGenerator` trait (mirroring `CommentaryGenerator`) that proposes `OverlayLink` candidates between prose and code nodes, each with `CitedSpan` evidence.
- Add a two-stage triage: cheap filter (name match + graph distance) before expensive LLM verification, to keep generation affordable on real repos.
- Activate `insert_link` / `links_for` on `SqliteOverlayStore`, with a new `cross_links` table physically isolated from graph tables and from the commentary table (new schema namespace only).
- Implement the confidence scoring model: span count, LCS ratio per span, span length, graph distance; produce three tiers (`high`, `review_queue`, `below_threshold`) with defined surfacing behavior.
- Wire `SymbolCard.proposed_links` and `FileCard.proposed_links` fields (Deep budget only, labeled as overlay-backed with freshness + confidence tier). Never merge into structural edge fields.
- Add review/promotion workflow: `synrepo links review` CLI (list, accept, reject) that writes accepted links to the graph as `HumanDeclared` edges and records outcomes to an audit-trail table. Curated mode only; no auto-promotion.
- Add drift-repair surface `proposed_links_overlay` with drift classes (`stale`, `missing_evidence`, `source_deleted`) and repair action `revalidate_links` in `src/pipeline/repair/`.
- Add `synrepo findings` CLI + `synrepo_findings` MCP tool that surfaces contradiction candidates (prose says X, code says not-X) and below-threshold links for operator audit.
- Persist audit trail for all candidate lifecycle events (generation, score changes, review decisions, promotion/rejection). Audit records survive candidate deletion.

## Capabilities

### New Capabilities
- `cross-link-store`: storage-layer contract for the overlay `cross_links` table — physical DB location, required on-disk fields per candidate, confidence tier enum, write isolation from graph + commentary tables, `prune_orphans` semantics when endpoint nodes vanish, audit-trail persistence.

### Modified Capabilities
- `overlay-links`: extend product contract with two-stage triage scope, below-threshold withholding semantics for card responses, and the explicit boundary that surfaced candidates never modify structural card fields.
- `cards`: add `proposed_links` field to `SymbolCard` and `FileCard` at Deep budget with confidence-tier labeling; specify that `proposed_links` is omitted at `tiny`/`normal` tiers and labeled `links_state: "budget_withheld"` for disambiguation from absent.
- `repair-loop`: add `proposed_links_overlay` repair surface with drift classes and the `revalidate_links` action; specify that revalidation does not touch the graph store.
- `mcp-surface`: add `synrepo_findings` tool contract — returns contradictions and below-threshold candidates with provenance, freshness, and audit fields.

## Impact

- **Code**: `src/overlay/` (types already present; no changes expected), `src/store/overlay/` (add `cross_links.rs` module, extend `schema.rs`), `src/pipeline/synthesis/` (add `cross_link.rs` with trait + Claude impl), `src/pipeline/repair/` (new surface + action), `src/surface/card/` (add `proposed_links` wiring in `compiler/symbol.rs` and `compiler/file.rs`, plus `types.rs`), `src/bin/cli.rs` + `src/bin/cli_support/commands.rs` (add `synrepo links` and `synrepo findings` subcommands), `crates/synrepo-mcp/src/main.rs` (add `synrepo_findings` tool).
- **Storage**: new `cross_links` table and `cross_link_audit` table in `.synrepo/overlay/overlay.db`. Schema-namespace-isolated from `commentary` table. No graph store schema changes.
- **Config**: new `cross_link_cost_limit` and `cross_link_confidence_threshold` fields in `Config`; compat-sensitive (changing threshold triggers overlay advisory, not rebuild).
- **Dependencies**: no new crates expected; reuse `rusqlite`, existing Claude API client in `src/pipeline/synthesis/claude.rs`.
- **Invariants preserved**: cross-links never enter the graph directly; synthesis still reads only `source_store = "graph"`; promotion from overlay to graph requires explicit human action and produces a `HumanDeclared` edge with full audit record.
