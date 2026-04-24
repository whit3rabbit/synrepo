## Context

Degradation is a product strength only if users can see what degraded and what still works. Today, separate subsystems report partial states differently. The matrix should normalize those states without changing their underlying ownership.

## Goals / Non-Goals

**Goals:**

- Define common readiness labels for supported, degraded, unavailable, disabled, stale, and blocked states.
- Attach next actions to each visible degradation.
- Reuse existing subsystem diagnostics.
- Keep optional features from blocking core graph-backed operation.

**Non-Goals:**

- No change to parser recovery strategy.
- No automatic dependency installation.
- No new embedding requirement.
- No watch daemon requirement for normal CLI use.

## Decisions

1. **Make runtime probe the aggregation boundary.** Probe and status already classify repo readiness. The matrix extends those outputs rather than letting each renderer invent labels.

2. **Distinguish disabled from unavailable.** A user who intentionally disables embeddings or watch should not see the same severity as a broken parser or compat-blocked store.

3. **Carry next actions as data.** Renderers can phrase text differently, but the matrix entry must name the recommended command or action.

4. **Do not downgrade graph truth because soft surfaces fail.** Overlay, embeddings, and watch degradation can affect convenience and ranking, but parser-observed graph facts keep their own correctness boundary.

## Risks / Trade-offs

- A broad matrix can become a second status implementation, mitigation: require source subsystem ownership for each row.
- Optional features can look scary if labeled poorly, mitigation: severity policy distinguishes disabled, degraded, and blocked.
- New labels can become API surface, mitigation: keep names stable and covered by tests.
