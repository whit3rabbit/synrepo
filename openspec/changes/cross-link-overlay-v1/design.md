## Context

Track K in ROADMAP.md. `commentary-overlay-v1` proved the overlay architecture: product-layer contract in `src/overlay/mod.rs`, storage-layer contract in `src/store/overlay/`, synthesis boundary as a narrow trait, content-hash freshness, reconcile-time pruning, and a repair-loop surface. Cross-links are structurally similar but diverge on three axes: (1) a candidate has *two* endpoints rather than one annotated node, (2) freshness depends on both endpoints' content hashes (not one), and (3) a human-review/promotion step exists that commentary does not have.

Current state:
- `OverlayLink`, `OverlayEdgeKind` (References/Governs/DerivedFrom/Mentions), `CitedSpan`, `OverlayEpistemic` types are defined in `src/overlay/mod.rs` but the `insert_link` / `links_for` trait methods on `OverlayStore` are stubs that return empty/error.
- `overlay-links/spec.md` captures the durable product contract (4 requirements). No storage-layer spec exists yet.
- `repair-loop/spec.md` explicitly reports the cross-link review surface as `unsupported` — that scenario comes out when this change lands.
- Synthesis scaffold (`src/pipeline/synthesis/`) has `CommentaryGenerator` trait + Claude + NoOp impls. The pattern is reusable.

## Goals / Non-Goals

**Goals:**
- Wire candidate generation, verification, confidence scoring, storage, review workflow, and audit trail end-to-end behind the existing overlay trait boundary.
- Keep the graph store structurally incapable of receiving a cross-link except through an explicit human-declared promotion path.
- Add `proposed_links` as an optional Deep-budget field on `SymbolCard` and `FileCard` with confidence-tier labeling.
- Ship a `synrepo findings` CLI + `synrepo_findings` MCP tool for contradiction surfacing.
- Add a `proposed_links_overlay` repair surface with deterministic `revalidate_links` action (re-run the evidence verifier, not the LLM).
- Preserve the invariant that synthesis reads only `source_store = "graph"`.

**Non-Goals:**
- Graph-level `CoChangesWith` edges. Separate roadmap item.
- Symbol-level last-change summaries. Separate roadmap item.
- Extra file/language classes. Milestone 6.
- Full daemon mode. Milestone 6.
- Automatic promotion of high-confidence candidates into the graph. Promotion is always explicit and curated-mode-only.
- Cross-links between nodes in unrelated repositories (multi-repo work is post-v1).

## Decisions

### D1: Two-stage triage before LLM verification
**Decision**: Candidate generation runs a cheap filter first (name/identifier match between prose chunks and symbol qualified names, graph distance ≤ N), and only the filtered set is sent to the LLM for evidence extraction.

**Rationale**: Unbounded LLM calls per (prose × symbol) pair is infeasible. The name-match prefilter is deterministic and fast, and a graph-distance cutoff prevents fan-out across unrelated modules. The cost is missing candidates whose phrasing doesn't share a token with the symbol name; those can be surfaced later through a slower "deep pass" if demand materializes.

**Alternatives considered**:
- Pure LLM-first (too expensive, non-deterministic cost on large repos)
- Embedding similarity prefilter (adds a vector store dependency; defer)
- Manual-only candidate authoring (defeats the purpose)

### D2: Confidence as a tier enum, not a float
**Decision**: The surfaced confidence is one of `high`, `review_queue`, `below_threshold`. Internally, a float score (0.0–1.0) is computed from span count, LCS ratio per span, average span length, and graph distance, but only the tier is exposed in card responses.

**Rationale**: Agents make worse decisions with unbounded floats than with named tiers. The `overlay-links` spec already defines tier-based surfacing behavior, and tiers let us move thresholds without changing the response schema. The float is preserved on disk for audit and threshold tuning.

**Alternatives considered**:
- Expose the float (leaks implementation detail; invites cargo-cult thresholds in agents)
- More tiers (`medium` adds noise; users need "surface to agent" vs "queue for review" vs "hide")

### D3: Two tables — `cross_links` and `cross_link_audit`
**Decision**: Active candidates live in a `cross_links` table keyed on `(from_node, to_node, kind)`. Every lifecycle event (creation, score change, review decision, promotion, rejection) writes an immutable row to a `cross_link_audit` table. The audit table survives candidate deletion.

**Rationale**: Review outcomes must be inspectable forever ("why was this promoted?"); reopening a rejected candidate must show the rejection history. Single-table with soft-delete conflates active and historical state and breaks the "non-authoritative until promoted" boundary. Commentary chose a single table because there is no review lifecycle to audit.

**Alternatives considered**:
- Event-sourced only (rebuild current state from audit log — adds read cost; wrong tradeoff for a frequently-queried card field)
- Soft-delete on a single table (complicates freshness queries; easier to forget the WHERE clause)

### D4: Freshness = both endpoints' hashes match at generation time
**Decision**: A candidate stores `from_content_hash` and `to_content_hash` captured at generation. `fresh` requires both to match the current graph; `stale` is any mismatch; `source_deleted` is when either endpoint no longer exists in the graph; `invalid` is missing provenance; `below_threshold` is a confidence state, not a freshness state.

**Rationale**: Freshness mirrors commentary's five-state model but doubled for the two-endpoint topology. Splitting "source deleted" from "stale" lets the repair loop act differently: stale → revalidate; source_deleted → prune.

**Alternatives considered**:
- Use only graph revision number (too coarse; a single-file edit invalidates everything)
- Track only one endpoint hash (misses code-side edits when prose is stable)

### D5: `revalidate_links` is deterministic, not LLM-backed
**Decision**: The `revalidate_links` repair action re-runs the fuzzy LCS verifier against stored `CitedSpan`s and the *current* source text of both endpoints. If all spans still verify above threshold, the candidate is marked fresh with updated hashes. If not, the candidate's freshness becomes `stale_evidence` and it drops out of surfacing until a full `synrepo sync --regenerate` is requested.

**Rationale**: Most "source changed" events don't actually invalidate cited spans — they change nearby code. Deterministic re-verification is free; LLM regeneration is expensive. Keeping the repair action cheap matches the commentary pattern where `NoOpGenerator` degrades gracefully when no API key is configured.

**Alternatives considered**:
- Always regenerate on any hash mismatch (expensive, wasteful on small edits)
- Mark stale and wait for explicit refresh (loses the cheap repair win)

### D6: Promotion writes a graph edge with `Epistemic::HumanDeclared`
**Decision**: `synrepo links accept <id>` writes a real graph edge (kind mapped from `OverlayEdgeKind` → `EdgeKind`) with `Epistemic::HumanDeclared` provenance pointing at the reviewer identity and the accepted candidate's audit-trail row. The overlay candidate row stays (for audit), with a `promoted_at` + `graph_edge_id` back-reference.

**Rationale**: This is the only path by which overlay content can influence the graph, and it must leave a trail in both stores. The type-system invariant (overlay epistemic vs graph epistemic are disjoint enums) means code can't accidentally write overlay provenance into the graph edge; the promotion function explicitly constructs a `HumanDeclared` variant.

**Alternatives considered**:
- Delete the overlay row on promotion (breaks audit trail — user might later want to see original evidence)
- Keep promoted links only in overlay (breaks the "graph is runtime truth" contract)

### D7: Curated-mode gating for review workflow
**Decision**: `synrepo links review/accept/reject` commands work only in `curated` mode. In `auto` mode, cross-links are generated and surfaced (at `high` tier), but no review/promotion surface is exposed.

**Rationale**: Curated mode already owns human-declared graph content in the existing pattern-surface work. Auto-mode users get value (agent-visible high-confidence links) without being forced into a maintenance workflow. Matches the existing mode split.

**Alternatives considered**:
- Review in both modes (adds a surface auto-mode users will never use)
- Review in auto only (inverts the mental model; curated is where humans curate)

### D8: Reuse the `CommentaryGenerator` pattern for the LLM boundary
**Decision**: Add `CrossLinkGenerator` trait in `src/pipeline/synthesis/cross_link.rs` with `generate_candidates(&self, scope: &CandidateScope) -> Result<Vec<OverlayLink>>`. Provide `ClaudeCrossLinkGenerator` (real) and `NoOpCrossLinkGenerator` (fallback when no API key).

**Rationale**: Keeps the synthesis pipeline swappable and testable, mirrors commentary's shape, and lets us ship the full surface (store + cards + repair) even when the LLM is absent.

### D9: `cross-link-store/spec.md` is a new sibling spec to `commentary-store/spec.md`
**Decision**: Create `openspec/specs/cross-link-store/spec.md` as the storage-layer contract. `overlay-links/spec.md` stays as the behavioral contract. Same layering as `overlay/spec.md` ↔ `commentary-store/spec.md`.

**Rationale**: The two-layer split proved its worth in commentary (clean separation of "what it means" from "how it's stored"). Replicate.

## Risks / Trade-offs

- **[Risk] LLM API cost on large repos** → Mitigation: two-stage triage (D1), per-run cost limit (`cross_link_cost_limit` config), `NoOpGenerator` fallback.
- **[Risk] False high-confidence candidates mislead agents** → Mitigation: tier thresholds tunable without schema change (D2); below-threshold candidates stay in `synrepo findings` for operator audit; all surfaced candidates are labeled overlay-backed and non-authoritative.
- **[Risk] Review UX is heavy and curated-mode users abandon it** → Mitigation: CLI-first surface (no TUI complexity); `synrepo links review --limit 10` returns a triaged batch; accept/reject are single-command operations; keyboard-driven.
- **[Risk] Audit-trail table grows unbounded** → Mitigation: separate table from hot path (D3); document retention policy (keep indefinitely by default; operator can `VACUUM` via a maintenance command in a later change); size projection before implementation.
- **[Risk] Promotion writes a graph edge whose provenance still points at an LLM-generated candidate** → Mitigation: D6 makes `HumanDeclared` epistemic mandatory; reviewer identity is required on promotion; the audit row on the overlay side preserves the original machine provenance; no code path exists that writes a non-`HumanDeclared` edge from overlay content (type-system guarantee).
- **[Risk] Deterministic revalidation (D5) silently misses meaningful source drift** → Mitigation: stored hashes still update on revalidate so subsequent stale detection is accurate; a separate `synrepo sync --regenerate-cross-links` path triggers full LLM re-run for operators who want it.
- **[Trade-off] Two-endpoint freshness doubles the hash-comparison cost** → Accepted; still O(1) per candidate, negligible vs LLM cost.
- **[Trade-off] `high` tier candidates surface in auto mode without human review** → Accepted; the tier threshold is conservative by default (LCS ≥ 0.95 across all spans, graph distance ≤ 2, ≥ 2 spans). Tunable via config.

## Migration Plan

1. Schema migration: add `cross_links` and `cross_link_audit` tables to overlay DB. Compatibility policy: overlay store version bump from `v1` (commentary-only) to `v2` (commentary + cross-links). Existing overlay DBs trigger a migration that runs `CREATE TABLE IF NOT EXISTS` only; no existing rows move.
2. Config bump: add `cross_link_cost_limit` (default 200 candidates per run) and `cross_link_confidence_threshold` with per-tier values. Compat check: changing the threshold produces an overlay advisory, not a rebuild.
3. Rollback: cross-link tables can be dropped without touching commentary or graph data. The `synrepo links revoke-promotion <edge-id>` command deletes a promoted graph edge + updates the overlay audit row (curated mode only). Operators who never ran `synrepo sync --generate-cross-links` have an empty `cross_links` table and nothing to clean up.
4. Staged enablement: land storage + repair-surface wiring in PR 1 (inert, surface reports `absent`); land candidate generation + card wiring in PR 2 (auto-generating in auto mode); land review/promotion in PR 3 (curated only).

## Open Questions

- Should `proposed_links` on `FileCard` include only cross-links where the file is an endpoint, or also cross-links that touch any symbol defined in the file? (leaning: file is the endpoint only; symbol-scoped links belong on `SymbolCard`)
- Should `synrepo_findings` MCP tool return contradictions (prose asserts X but code demonstrates ¬X) in this change, or defer to a follow-on? (leaning: surface-only in this change; contradiction detection is a real synthesis problem worth its own scope; stub the field now with `contradictions: []` and a TODO note)
- What is the right default for `cross_link_cost_limit` on a fresh repo? (need a small benchmark: run against synrepo itself, see how many prose↔code candidates the prefilter yields; pick a limit that covers the top 2 quartiles)
