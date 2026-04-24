## Context

The current workflow aliases make the happy path possible. The remaining problem is behavioral: agents can still treat synrepo as optional and read files cold. Synrepo can not police every external read, but it can make the intended sequence explicit and measure the calls it serves.

## Goals / Non-Goals

**Goals:**

- Make the workflow doctrine blunt and consistent across generated shims, MCP info, and tool descriptions.
- Treat full-file reads as an explicit escalation after a bounded card or minimum-context result identifies the target.
- Track observable workflow signals such as orient calls, card calls, impact calls, test calls, changed calls, and file-card raw-token comparisons.
- Keep minimum-context positioned as the default bounded neighborhood step.

**Non-Goals:**

- No filesystem sandbox or read interception.
- No prompt logging.
- No requirement that every client implement enforcement.
- No change to graph or overlay truth boundaries.

## Decisions

1. **Doctrine is the source of workflow language.** Generated shims and MCP descriptions should reuse short canonical phrases to avoid drift.

2. **Metrics measure observable calls, not private agent behavior.** Synrepo can count served cards and workflow tool usage. It cannot claim avoided reads that did not go through synrepo unless the estimate is clearly derived.

3. **Minimum-context is the bridge to deep inspection.** Agents should use it before deep cards or full source reads when the relevant target is known but neighborhood risk is unclear.

4. **Keep guidance strong but reversible.** The hardening improves agent behavior without blocking legitimate direct reads when cards are insufficient.

## Risks / Trade-offs

- Strong doctrine can become annoying if too verbose, mitigation: keep generated text short and put detail in the skill file.
- Cold-read avoidance is partly estimated, mitigation: label estimates and separate them from observed counters.
- Tool descriptions can bloat MCP listings, mitigation: use one short workflow sentence per relevant tool.
