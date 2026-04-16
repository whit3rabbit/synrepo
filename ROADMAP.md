# ROADMAP

> Remaining work only. Completed changes are archived in `openspec/changes/archive/`.
> Architecture, invariants, and shipped surface are documented in `AGENTS.md`.

## Active OpenSpec Changes

8 changes are staged in `openspec/changes/`. They are grouped into 2 execution phases — each phase depends on the one before it.

### Phase 1 — Structural Completion

| Change | Purpose | Tracks | Status |
|--------|---------|--------|--------|
| [`graph-cochange-edges-v1`](openspec/changes/graph-cochange-edges-v1/) | Emit physical `CoChangesWith` graph edges from git history (kind defined but never produced today) | D, I | Shipped |
| [`symbol-last-change-v1`](openspec/changes/symbol-last-change-v1/) | True symbol-level `last_change` tracking via `body_hash` transitions; upgrades `SymbolCard.last_change.granularity` from `file` → `symbol` | D, I | Shipped |
| [`structural-resilience-v1`](openspec/changes/archive/2026-04-15-structural-resilience-v1/) | Stage 6 split/merge + Stage 7 drift scoring: **infrastructure shipped** (types, tables, wiring, repair surface). Semantics incomplete; see v2. | D | Shipped |
| [`structural-resilience-v2`](openspec/changes/structural-resilience-v2/) | Finish drift scoring (Jaccard distance on persisted fingerprints, all-edge enumeration, concept edges), wire git rename fallback (cascade step 4), fix repair absent-vs-current | D | Shipped |
| [`graph-lifecycle-v1`](openspec/changes/graph-lifecycle-v1/) | Stable identity + owned observations + soft retirement: replace destructive rebuild with scoped refresh, add compile revisions, compaction maintenance pass | D | In progress |

**Why first:** These fill the remaining data gaps in the graph. Phase 2 surfaces consume this data.

---

### Phase 2 — Surface Expansion & Ops

| Change | Purpose | Tracks | Status |
|--------|---------|--------|--------|
| [`specialist-cards-v1`](openspec/changes/specialist-cards-v1/) | `CallPathCard` + `synrepo_call_path`, `TestSurfaceCard` + `synrepo_test_surface` | E | Shipped |
| [`semantic-triage-v1`](openspec/changes/semantic-triage-v1/) | ONNX/MiniLM embedding pre-filter for cross-link candidate generation (opt-in, feature-gated) | K | In progress |
| [`storage-compaction-v1`](openspec/changes/storage-compaction-v1/) | `synrepo compact` command; policy-driven retention for overlay, repair-log, audit rows | H, L | In progress |

**Why second:** Consumes the enriched graph data from Phase 1. Specialist cards need co-change and drift; compaction needs the overlay to have aged enough to need it.

---

## Future Changes (not yet spec'd)

These open after the 6 active changes are archived:

| Change | Purpose | Depends on |
|--------|---------|------------|
| `change-risk-card-v1` | `ChangeRiskCard` + `synrepo_change_risk` consuming drift scores, co-change edges, and hotspot data | Phase 1 (drift, co-change) |
| `workflow-handoffs-v1` | Derived `synrepo_next_actions` MCP tool + `synrepo handoffs` CLI; reads repair report, overlay candidates, git hotspots — no new mutable store | Phase 2 (compaction, specialist cards) |

---

## Remaining Structural Pipeline Gaps

These are tracked by the active changes above but worth calling out:

| Pipeline stage | Status | Change |
|----------------|--------|--------|
| Stage 8 — ArcSwap commit | TODO stub | Not yet assigned |

## Shipped Cards & MCP Tools

| Surface | Status | Change |
|---------|--------|--------|
| `SymbolCard` / `synrepo_card` | Shipped | - |
| `FileCard` / `synrepo_card` (file target) | Shipped | - |
| `ModuleCard` / `synrepo_module_card` | Shipped | - |
| `EntryPointCard` / `synrepo_entrypoints` | Shipped | - |
| `PublicAPICard` / `synrepo_public_api` | Shipped | - |
| `DecisionCard` | Shipped | - |
| `MinimumContextCard` / `synrepo_minimum_context` | Shipped | - |
| `CallPathCard` / `synrepo_call_path` | Shipped | `specialist-cards-v1` |
| `TestSurfaceCard` / `synrepo_test_surface` | Shipped | `specialist-cards-v1` |

## Not Yet Shipped Cards & MCP Tools

| Surface | Status | Change |
|---------|--------|--------|
| `ChangeRiskCard` / `synrepo_change_risk` | Not started | `change-risk-card-v1` (future) |
| `synrepo_explain` | Not started | Unassigned |

## Guiding Principles

1. Observed facts only in the graph; machine content only in the overlay.
2. The product is useful before any LLM synthesis exists.
3. Cards are the primary user-facing abstraction.
4. Smallest truthful context first.
5. Watch, reconcile, and repair are trust features, not polish.
6. synrepo is a repo context compiler, not a memory product or task tracker.

## Spec & Planning References

- Domain specs: `openspec/specs/`
- Active changes: `openspec/changes/`
- Archived changes: `openspec/changes/archive/`
- Architecture & invariants: `AGENTS.md`
- Foundation design: `docs/FOUNDATION.md`
- Product spec: `docs/FOUNDATION-SPEC.md`
