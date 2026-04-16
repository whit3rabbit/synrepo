# ROADMAP

> Remaining work only. Completed changes are archived in `openspec/changes/archive/`.
> Architecture, invariants, and shipped surface are documented in `AGENTS.md`.

## Active OpenSpec Changes

No changes are active. 8 changes have been archived.

## Shipped Changes

| Change | Purpose | Tracks |
|--------|---------|--------|
| [`graph-cochange-edges-v1`](openspec/changes/archive/2026-04-15-graph-cochange-edges-v1/) | Emit physical `CoChangesWith` graph edges from git history | D, I |
| [`symbol-last-change-v1`](openspec/changes/archive/2026-04-15-symbol-last-change-v1/) | True symbol-level `last_change` tracking via `body_hash` transitions | D, I |
| [`structural-resilience-v1`](openspec/changes/archive/2026-04-15-structural-resilience-v1/) | Stage 6 split/merge + Stage 7 drift scoring infrastructure | D |
| [`structural-resilience-v2`](openspec/changes/archive/2026-04-15-structural-resilience-v2/) | Finish drift scoring, wire git rename fallback, fix repair absent-vs-current | D |
| [`graph-lifecycle-v1`](openspec/changes/archive/2026-04-16-graph-lifecycle-v1/) | Stable identity + owned observations + soft retirement | D |
| [`specialist-cards-v1`](openspec/changes/archive/2026-04-15-specialist-cards-v1/) | `CallPathCard` + `TestSurfaceCard` | E |
| [`semantic-triage-v1`](openspec/changes/archive/2026-04-16-semantic-triage-v1/) | ONNX/MiniLM embedding pre-filter for cross-link candidate generation | K |
| [`change-risk-card-v1`](openspec/changes/archive/2026-04-16-change-risk-card-v1/) | `ChangeRiskCard` consuming drift scores, co-change edges, hotspots | M |
| [`storage-compaction-v1`](openspec/changes/archive/2026-04-15-storage-compaction-v1/) | `synrepo compact` command; policy-driven retention | H, L |

---

## Future Changes (not yet spec'd)

These open after the remaining active change is shipped:

| Change | Purpose | Depends on |
|--------|---------|------------|
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
| `ChangeRiskCard` / `synrepo_change_risk` | Shipped | `change-risk-card-v1` |

## Not Yet Shipped Cards & MCP Tools

| Surface | Status | Change |
|---------|--------|--------|
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
