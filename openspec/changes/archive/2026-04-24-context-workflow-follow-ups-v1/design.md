## Context

The archived `context-accounting-and-workflow-v1` change landed the context-budget contract, workflow aliases, and metrics plumbing. Three follow-ups from the review remained unscoped:

- A `synrepo_risks` shorthand for `synrepo_impact` (the review used "risks" as the natural verb for the pre-edit impact check).
- Health-surface visibility for the two most actionable context metrics: `estimated_tokens_saved_total` (the positive signal agents and operators care about) and `stale_responses_total` (the degradation signal that must escalate).
- Doctrine and skill text that tell agents both names exist.

This change closes those three gaps without touching the deterministic graph, the explain pipeline, or the overlay contract.

## Goals / Non-Goals

**Goals:**
- Give the workflow "risks before edit" a short command name without duplicating handler logic.
- Make tokens-avoided and stale-responses visible on the same Health tab that already surfaces `context` cards-served and tokens/card.
- Keep doctrine wording aligned across the macro, shims, and `skill/SKILL.md`.

**Non-Goals:**
- No new metric fields on `ContextMetrics`; both values already exist at `src/pipeline/context_metrics.rs`.
- No new handler logic for `synrepo_risks`. It dispatches to the same `cards::handle_change_risk` as `synrepo_impact`, taking the same `ChangeRiskParams`.
- No overlay, explain, or graph behavior changes.

## Decisions

1. **`synrepo_risks` shares `ChangeRiskParams` with `synrepo_impact`.** Byte-identical output means no duplicated schema, no drift between the two names, and a single source-scan test can enforce that both are registered.

2. **Stale-responses row escalates severity when non-zero.** `tokens avoided` stays `Healthy` because it is a positive signal; `stale responses` flips to `Stale` on any non-zero count so the dashboard visibly nudges the operator.

3. **Rows sit next to the existing `context` row.** The three rows (`context`, `tokens avoided`, `stale responses`) share `snapshot.context_metrics` as their source, so they appear together or not at all. When metrics are absent the whole group is omitted rather than showing zeros.

4. **Doctrine names both aliases in one sentence.** Step 4 of the default path becomes "Use `synrepo_impact` (or its shorthand `synrepo_risks`) before editing." The macro propagates to every shim, and `skill/SKILL.md` is edited to match.

## Risks / Trade-offs

- An extra MCP tool increases registration weight minimally. Mitigated by zero new handler code.
- Doctrine drift is possible if `skill/SKILL.md` and the macro diverge. The existing `skill_md_includes_doctrine_lines_verbatim` test already guards parts of this, and the source-scan test in `src/bin/cli_support/tests/mcp.rs` covers the registration side.
