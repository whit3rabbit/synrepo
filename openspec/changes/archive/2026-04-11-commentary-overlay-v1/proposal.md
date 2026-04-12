## Why

The current `openspec/specs/overlay/spec.md` bundles commentary provenance and freshness together with evidence-verified cross-link generation and review surfaces. These are two distinct capabilities at different readiness levels, and implementing Milestone 5 straight from the current spec risks over-scoping the first overlay slice before the commentary foundation is stable. Separating them now keeps the contract clean before any implementation code is written.

## What Changes

- Narrow `openspec/specs/overlay/spec.md` to commentary-only: storage, provenance, freshness states, retrieval boundaries, and audit exposure. Remove cross-link requirements from this spec.
- Add `openspec/specs/overlay-links/spec.md` as the new durable home for evidence-verified cross-link candidate generation, verification, confidence scoring, and review surfaces (Track K).
- Update `ROADMAP.md` section 7.12 to decouple Track J and Track K: Track J governs `overlay/spec.md` (commentary); Track K governs `overlay-links/spec.md` (cross-links).
- Tighten `openspec/specs/mcp-surface/spec.md` so the provenance-and-freshness requirement explicitly defines what happens when commentary is present, stale, missing, or budget-withheld — not just "overlay-backed content" in the abstract.
- Clarify `openspec/specs/repair-loop/spec.md` so the stale-overlay-entries surface is explicitly scoped to commentary overlay; cross-link review surfaces remain unsupported until `cross-link-overlay-v1` lands.

## Capabilities

### New Capabilities

- `overlay-links`: Evidence-verified cross-link candidate generation, verification, confidence scoring, human review workflow, proposed-link promotion, and contradiction audit trail. Defers Track K behavior out of the commentary overlay contract into its own durable spec so it can be implemented later without reopening `overlay/spec.md`.

### Modified Capabilities

- `overlay`: Narrowed from "commentary and proposed links" to commentary-only. Removes the bounded-evidence-verified-linking requirement and adds explicit commentary freshness states (`fresh`, `stale`, `invalid`, `missing`, `unsupported`), minimum provenance fields, retrieval boundaries, and cost controls. Cross-link requirements move to `overlay-links`.
- `mcp-surface`: Sharpens the provenance-and-freshness requirement to enumerate the overlay commentary states (present-fresh, present-stale, absent, budget-withheld) and define how each is labeled in MCP responses. The current requirement names "overlay-backed content" but leaves the per-state behavior undefined.

## Impact

- `openspec/specs/overlay/spec.md` — rewritten to commentary scope only
- `openspec/specs/overlay-links/spec.md` — new file
- `openspec/specs/mcp-surface/spec.md` — one requirement updated with commentary state enumeration
- `openspec/specs/repair-loop/spec.md` — one requirement updated to scope stale-overlay-entries to commentary overlay
- `ROADMAP.md` — sections 7.12 and 8.11 updated to decouple Track J from Track K
- No runtime code changes in this change; all modifications are to planning contracts and durable specs
