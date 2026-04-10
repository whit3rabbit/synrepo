## Context

The current codebase already leaves clear placeholders for Git intelligence. `src/config.rs` carries `git_commit_depth`, `src/structure/graph.rs` models `git_observed` epistemic authority and `CoChangesWith` edges, `src/pipeline/structural.rs` reserves a Git-mining stage, and `src/surface/card.rs` already includes `last_change` and `co_changes` fields. That is enough to establish a precise change without guessing at a product that does not exist yet.

This is the right next planning step because Track I is a milestone-driving feature, and without a dedicated change the implementation would have to improvise where Git-derived facts belong, how they are ranked, and what happens when history is incomplete.

This is an early planning change, not an instruction to bypass milestone order. Implementation should still follow the roadmap sequence: observed-facts core first, then Git-intelligence execution once the Milestone 2 substrate exists to support it.

## Goals / Non-Goals

**Goals:**
- Define deterministic Git-history mining outputs that synrepo treats as `git_observed` evidence.
- Specify how ownership, hotspots, co-change, and last meaningful change enrich cards and routing without becoming canonical descriptive truth.
- Define degraded-history behavior clearly enough that shallow clones and detached HEADs do not produce misleading outputs.
- Use the repo's existing Rust/Git direction rather than introducing a separate history subsystem.

**Non-Goals:**
- Implement cards-and-MCP Phase 2 behavior beyond the Git-derived fields and ranking inputs they need.
- Change overlay or synthesis behavior.
- Solve every future history heuristic such as bus factor or organization-wide ownership analytics.
- Rewrite graph identity or structural parse policy.

## Decisions

1. Git intelligence is deterministic and structural.
   It is mined from repository history and stored or surfaced as `git_observed` evidence. It is not an LLM-derived interpretation layer.

2. Git-derived evidence stays subordinate to parser-observed structure.
   History may affect ranking, explanation, and risk estimation, but it does not redefine what the code currently does.

3. The first output set is narrow and actionable.
   Ownership hints, hotspot signals, co-change relationships, and last meaningful change summaries are enough to improve routing and change-impact behavior without inventing a giant analytics layer.

4. Degraded-history behavior is explicit.
   Shallow clones, detached HEAD, missing blame context, ignored submodules, and rewritten history should surface as degraded Git intelligence rather than as silent partial truth.

5. Existing card fields are the first integration point.
   `FileCard`, `ChangeRiskCard`, and related ranking surfaces should receive Git-derived enrichment before new public surfaces are invented.

## Risks / Trade-offs

- Git history can be useful but noisy. If the contract is too loose, routing quality will depend on repository idiosyncrasies instead of stable semantics.
- Strong heuristics without degraded-state reporting would create false confidence in shallow or unusual repos.
- Keeping the first change narrow avoids analytics sprawl, but some future Git signals will still need later follow-on work.
