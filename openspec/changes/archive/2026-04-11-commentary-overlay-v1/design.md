## Context

`openspec/specs/overlay/spec.md` currently defines two conceptually separate capabilities under one spec:

1. Commentary overlay: storage, provenance, freshness, retrieval boundaries, and audit exposure for machine-authored annotations on graph nodes.
2. Evidence-verified cross-links: candidate generation, verification, confidence scoring, and human review workflow for proposed cross-node relationships.

These were co-located when the spec was first written because both live in the overlay store. However, commentary and cross-links have different readiness levels, different implementation costs, and different trust-model implications. Commentary can be implemented without any candidate-generation or human-review pipeline. Cross-links cannot be safely implemented without a verification pass and a promotion workflow, both of which are further out on the roadmap.

This change is a contracts-first reorganization that happens entirely in `openspec/`. No runtime code changes are included. The goal is to create clean boundaries before Milestone 5 implementation begins.

## Goals / Non-Goals

**Goals:**

- Narrow `openspec/specs/overlay/spec.md` to commentary-only requirements.
- Create `openspec/specs/overlay-links/spec.md` as the durable home for cross-link behavior (Track K).
- Update `ROADMAP.md` section 7.12 so Track J points at `overlay/spec.md` (commentary) and Track K points at `overlay-links/spec.md` (cross-links) — removing the current single-spec coupling.
- Tighten `openspec/specs/mcp-surface/spec.md` provenance-and-freshness requirement to enumerate overlay commentary states and define per-state MCP labeling behavior.
- Narrow the stale-overlay-entries surface in `openspec/specs/repair-loop/spec.md` to commentary overlay entries specifically, so cross-link review surfaces remain unsupported until `cross-link-overlay-v1`.

**Non-Goals:**

- No runtime Rust code changes. The existing `src/overlay/mod.rs` module boundary is already established and is not modified here.
- No implementation of commentary generation, synthesis pipeline, or any LLM-calling code.
- No changes to `overlay::OverlayEpistemic`, `overlay::OverlayStore`, or related runtime types. Those belong to the implementation change that follows.
- No changes to card output, MCP tool behavior, or CLI commands. The MCP spec delta in this change sharpens an existing requirement; it does not add new tool surface.
- No changes to `openspec/specs/overlay/spec.md`'s relationship to the graph trust boundary. The invariant that overlay content never overrides graph truth is preserved unchanged.

## Decisions

### Decision: Keep `overlay/spec.md` as the commentary home, not a rename

Alternatives considered:
- Rename `overlay/spec.md` to `commentary-overlay/spec.md` and start fresh.
- Keep the current name but reorganize sections in place.

Chosen: Keep `openspec/specs/overlay/spec.md` as the commentary home with its existing name. The existing references throughout `ROADMAP.md`, `CLAUDE.md`, and prior change archives all point to `overlay/spec.md`. Renaming adds churn without clarity gain. The spec's purpose line already says "commentary, proposed links, freshness, and review surfaces" — narrowing the scope in place is the smallest durable change.

### Decision: New spec file at `overlay-links/spec.md` rather than a sub-section

Alternatives considered:
- Keep cross-link requirements as a clearly-marked future-phase section within `overlay/spec.md`.
- Create `openspec/specs/cross-links-and-review/spec.md` with a broader scope.

Chosen: `openspec/specs/overlay-links/spec.md` with a focused scope on the cross-link pipeline. A separate file makes it unambiguous that Track K is a distinct delivery unit from Track J. Keeping cross-link requirements inside `overlay/spec.md` (even gated behind a phase comment) would invite scope creep when Milestone 5 implementation begins — reviewers would naturally ask "but what about the proposed-link requirement right here." The `overlay-links` name is parallel to `overlay` and signals the relationship without conflating the two.

### Decision: MCP surface change is a requirement modification, not a new requirement

The existing `mcp-surface/spec.md` already has a requirement named "Require provenance and freshness in responses." This change modifies that requirement to enumerate the four observable commentary states rather than adding a new requirement alongside it. Adding a second requirement for "overlay commentary states" would create overlap and ambiguity about which requirement governs which case. Modifying in place keeps the requirement surface clean.

### Decision: Repair-loop change is narrow — one requirement modified, not a new surface added

The existing `repair-loop/spec.md` requirement "Define targeted drift classes" names "stale overlay entries" as a drift surface. This change modifies that name to "stale commentary overlay entries" and adds a scenario that makes explicit what happens when cross-link review surfaces are not yet implemented. No new repair surface is added. Adding a separate `unsupported-cross-link-surface` requirement would be premature until `cross-link-overlay-v1` defines what a cross-link review surface actually is.

## Risks / Trade-offs

- **Risk: `overlay-links/spec.md` over-specifies Track K prematurely** → Mitigation: The new spec captures requirements at the same level of abstraction as `overlay/spec.md` (behavioral scenarios, not implementation detail). It does not define data schemas, pipeline stages, or algorithm choices — those belong in the `cross-link-overlay-v1` design.
- **Risk: MCP spec delta creates inconsistency with the runtime tool contracts** → Mitigation: The MCP delta sharpens existing behavior and does not add new tool parameters or response fields. The current MCP tool implementations already return `source_store` labels; this requirement change documents what the label values mean for commentary states.
- **Risk: ROADMAP.md section 7.12 diverges from the specs after future changes** → Mitigation: Section 7.12 is updated in this change to point Track J at `overlay/spec.md` and Track K at `overlay-links/spec.md` with separate entries. Future changes against either spec will reference their own governing section cleanly.

## Open Questions

None. All decisions are resolved above. The implementation sequence for Milestone 5 is: ship `commentary-overlay-v1` runtime work first, then open `cross-link-overlay-v1` separately once commentary storage is proven stable.
