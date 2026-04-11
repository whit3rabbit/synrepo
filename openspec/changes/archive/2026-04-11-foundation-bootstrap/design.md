## Context

synrepo already has strong foundation thinking in `ROADMAP.md`, `docs/FOUNDATION.md`, and `docs/FOUNDATION-SPEC.md`, and the repo now has the first durable OpenSpec spine and active `foundation-bootstrap` change. The remaining planning problem is no longer missing folders. It is contract sharpness: several release-driving behaviors are still clearer in docs and runtime code than in the durable specs.

The design goal here is not to implement product features. It is to create the contract surface that prevents future feature changes from improvising their own structure.

## Goals / Non-Goals

**Goals:**
- Establish the durable `openspec/specs/<capability>/spec.md` structure listed in `ROADMAP.md`.
- Make `foundation` the anchor capability and split the remaining concerns into clear ownership boundaries.
- Tighten the initial spine where runtime behavior already outpaces spec precision, especially around storage compatibility, git intelligence, exports/views, provenance, and bootstrap semantics.
- Create a single active bootstrap change that explains why the foundation exists and how future changes should build on it.
- Reuse existing foundation documents as source material instead of rewriting the product story from scratch.
- Lock the repository boundary rules for `openspec/specs/`, `openspec/changes/`, `docs/`, and `.synrepo/`.

**Non-Goals:**
- Implement `synrepo init`, graph storage, cards, MCP tools, or any runtime behavior.
- Open feature changes beyond `foundation-bootstrap`.
- Archive this change or sync its deltas into another state automatically.
- Replace the existing docs as reference material in this pass.

## Decisions

1. The repo gets the full durable spec spine now.
   The roadmap already names the enduring capabilities, so delaying the empty or minimal specs would just push the structure argument into later feature changes.

2. `foundation` is the anchor capability.
   It owns product mission, trust boundaries, operating modes, and the rule that OpenSpec is a planning layer rather than runtime truth.

3. Existing docs are promoted by splitting, not copied wholesale.
   `docs/FOUNDATION-SPEC.md` supplies the highest-level product and acceptance material.
   `docs/FOUNDATION.md` supplies deeper architecture and operational detail.
   The new specs are shorter and requirement-oriented so future changes can modify them cleanly.

4. Repository boundaries are explicit.
   `openspec/specs/` is for durable behavior.
   `openspec/changes/` is for active proposals and deltas.
   `docs/` is supporting or exploratory narrative.
   `.synrepo/` is runtime state and caches.

5. The initial change stays foundation-only.
   Future roadmap items are named in sequence, but they are not pre-created as empty changes because that would look like progress without adding decision value.

6. The bootstrap change includes delta specs only for the capabilities that this change materially establishes as immediate governance surfaces.
   That is `foundation`, `bootstrap`, and `evaluation`, matching the roadmap's own recommendation for the first change.

7. Future change naming follows roadmap-aligned kebab-case.
   Expected next changes are `lexical-substrate-v1`, `bootstrap-ux-v1`, `structural-graph-v1`, `watch-reconcile-v1`, `cards-and-mcp-v1`, `git-intelligence-v1`, `pattern-surface-v1`, `repair-loop-v1`, `commentary-overlay-v1`, `cross-link-overlay-v1`, and `export-and-polish-v1`.

8. Three additional durable capabilities are justified now.
   `git-intelligence` gets its own spec because Track I is release-driving and was previously spread across graph and card language.
   `storage-and-compatibility` gets its own spec because `.synrepo/` layout, migration, rebuild, and config compatibility are already concrete runtime concerns.
   `exports-and-views` gets its own spec because Track L otherwise turns into a catch-all and because exports participate in freshness, repair, and contamination policy.

9. Provenance and auditability are first-class differentiators.
   The durable specs must explicitly define minimum provenance and degraded-audit behavior for graph and overlay artifacts, not just mention provenance in passing.

## Risks / Trade-offs

- Creating the full spec tree now adds some upfront writing cost, but it is cheaper than allowing each later change to define its own capability boundaries.
- The new specs remain concise, but they now need enough precision to stop later changes from re-inventing storage, export, bootstrap, and git-history policy.
- Because this workspace is not currently a Git repository, Git-based verification is unavailable in this pass. OpenSpec validation still works and is the main structural check.
