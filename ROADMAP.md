# ROADMAP

> Remaining work only. Completed changes are archived in `openspec/changes/archive/`.
> Architecture, invariants, and shipped surface are documented in `AGENTS.md`.

## Active OpenSpec Changes

8 changes are staged in `openspec/changes/`. They are grouped into 3 execution phases — each phase depends on the one before it.

### Phase 1 — Doctrine & Escape Hatches

| Change | Purpose | Tracks |
|--------|---------|--------|
| [`agent-doctrine-v1`](openspec/changes/agent-doctrine-v1/) | Rewrite SKILL.md and agent shims around a single escalation flow; embed do-not rules in MCP tool descriptions | E, L |
| [`mcp-primitives-v1`](openspec/changes/mcp-primitives-v1/) | Low-level raw graph MCP tools (`synrepo_node`, `synrepo_edges`, `synrepo_query`) for power users and escape-hatch debugging | E |

**Why first:** No storage or schema changes. Improves agent adoption quality immediately.

---

### Phase 2 — Structural Completion

| Change | Purpose | Tracks |
|--------|---------|--------|
| [`graph-cochange-edges-v1`](openspec/changes/graph-cochange-edges-v1/) | Emit physical `CoChangesWith` graph edges from git history (kind defined but never produced today) | D, I |
| [`symbol-last-change-v1`](openspec/changes/symbol-last-change-v1/) | True symbol-level `last_change` tracking via `body_hash` transitions; upgrades `SymbolCard.last_change.granularity` from `file` → `symbol` | D, I |
| [`structural-resilience-v1`](openspec/changes/structural-resilience-v1/) | Stage 6 split/merge detection (`SplitFrom`/`MergedFrom` edges) and Stage 7 drift scoring | D |

**Why second:** These fill the remaining data gaps in the graph. Phase 3 surfaces consume this data.

---

### Phase 3 — Surface Expansion & Ops

| Change | Purpose | Tracks |
|--------|---------|--------|
| [`specialist-cards-v1`](openspec/changes/specialist-cards-v1/) | `CallPathCard` + `synrepo_call_path`, `TestSurfaceCard` + `synrepo_test_surface` | E |
| [`semantic-triage-v1`](openspec/changes/semantic-triage-v1/) | ONNX/MiniLM embedding pre-filter for cross-link candidate generation (opt-in, feature-gated) | K |
| [`storage-compaction-v1`](openspec/changes/storage-compaction-v1/) | `synrepo compact` command; policy-driven retention for overlay, repair-log, audit rows | H, L |

**Why third:** Consumes the enriched graph data from Phase 2. Specialist cards need co-change and drift; compaction needs the overlay to have aged enough to need it.

---

## Future Changes (not yet spec'd)

These open after the 8 active changes are archived:

| Change | Purpose | Depends on |
|--------|---------|------------|
| `change-risk-card-v1` | `ChangeRiskCard` + `synrepo_change_risk` consuming drift scores, co-change edges, and hotspot data | Phase 2 (drift, co-change) |
| `workflow-handoffs-v1` | Derived `synrepo_next_actions` MCP tool + `synrepo handoffs` CLI; reads repair report, overlay candidates, git hotspots — no new mutable store | Phase 3 (compaction, specialist cards) |

---

## Remaining Structural Pipeline Gaps

These are tracked by the active changes above but worth calling out:

| Pipeline stage | Status | Change |
|----------------|--------|--------|
| Stage 6 — Identity cascade (split/merge) | Scaffold only | `structural-resilience-v1` |
| Stage 7 — Drift scoring | Scaffold only | `structural-resilience-v1` |
| Stage 8 — ArcSwap commit | TODO stub | Not yet assigned |
| `CoChangesWith` edges | Kind defined, never emitted | `graph-cochange-edges-v1` |
| Symbol-level `last_change` | File-level proxy today | `symbol-last-change-v1` |

## Not Yet Shipped Cards & MCP Tools

| Surface | Status | Change |
|---------|--------|--------|
| `CallPathCard` / `synrepo_call_path` | Not started | `specialist-cards-v1` |
| `TestSurfaceCard` / `synrepo_test_surface` | Not started | `specialist-cards-v1` |
| `ChangeRiskCard` / `synrepo_change_risk` | Not started | `change-risk-card-v1` (future) |
| `synrepo_explain` | Not started | Unassigned |
| `synrepo_node`, `synrepo_edges`, `synrepo_query` | Not started | `mcp-primitives-v1` |

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
