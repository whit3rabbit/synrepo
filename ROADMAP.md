# ROADMAP

> Remaining work only. Completed changes are archived in `openspec/changes/archive/`.
> Architecture, invariants, and shipped surface are documented in `AGENTS.md`.

## Active OpenSpec Changes

| Change | Purpose | Status |
|--------|---------|--------|
| [`workflow-handoffs-v1`](openspec/changes/workflow-handoffs-v1/) | Derived `synrepo_next_actions` MCP tool + `synrepo handoffs` CLI; reads repair report, overlay candidates, git hotspots — no new mutable store | In progress |
| `onboarding-overhaul-v1` | `synrepo setup <client>` command, OpenCode support, and clean binary-first workflow | Shipped |

---

## Future Changes (not yet spec'd)

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
