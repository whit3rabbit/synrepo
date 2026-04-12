## Context

The current runtime already has three useful pieces of repair-adjacent behavior:

- `status` exposes runtime diagnostics through `src/pipeline/diagnostics.rs`.
- storage compatibility and cleanup decisions are centralized in `src/pipeline/maintenance.rs`.
- structural refresh already has a canonical write path in `run_reconcile_pass()` inside `src/pipeline/watch.rs`.

It also has a shipped read surface that should stay untouched by this change: CLI commands for status/search/graph inspection, plus the five core MCP tools (`synrepo_overview`, `synrepo_card`, `synrepo_search`, `synrepo_where_to_edit`, `synrepo_change_impact`). `repair-loop-v1` adds `check` and `sync`; it does not redefine the existing card or MCP contracts.

What is missing is a single repair surface that can inspect stale state across these systems, classify the problem precisely, and apply only the necessary deterministic refresh steps. That gap becomes more important in Milestone 4 because `pattern-surface-v1` will add human-guidance links and rationale surfaces that can drift independently from structural facts.

There is also an important timing constraint: overlay and generated export systems are not fully implemented yet. `repair-loop-v1` therefore needs to define first-class behavior for surfaces that are absent or not yet materialized, rather than pretending every repair target already exists.

## Goals / Non-Goals

**Goals:**
- Expose a deterministic, read-only `synrepo check` command that reports stale or unhealthy surfaces in a machine-readable and human-readable form.
- Expose a targeted `synrepo sync` command that repairs only deterministic, selected surfaces by reusing existing producer and maintenance paths.
- Define a stable repair finding model with surface kind, drift class, target identity, recommended action, and outcome.
- Make unsupported or not-yet-materialized surfaces visible in repair reports without turning them into silent no-ops.
- Record an audit trail of repair runs so local users and CI can inspect what was detected and what actions were taken.

**Non-Goals:**
- Implement overlay generation, cross-link verification, or export synthesis that does not already exist.
- Auto-edit human-authored docs, ADRs, or pattern files.
- Create a second write path for graph refreshes outside `run_reconcile_pass()` and the existing maintenance layer.
- Solve every future repair heuristic, such as semantic rationale decay or organization-wide policy enforcement.

## Decisions

### Decision 1: The repair loop is surface-oriented

The repair system will classify drift against named surfaces such as runtime views, exports, declared rationale links, overlay entries, and trust boundaries, not against arbitrary graph nodes in isolation.

Why: users repair stale surfaces, not abstract graph rows. This keeps the CLI understandable and aligns with the roadmap wording.

Alternative considered: a node-level repair engine that emits findings per node and edge. Rejected because it would expose too much storage detail in the first user-facing repair workflow and make selective repair harder to reason about.

### Decision 2: `check` is read-only and `sync` is the only mutating repair entry point

`synrepo check` will only inspect and classify drift. `synrepo sync` will re-evaluate the requested scope and then execute deterministic repairs for auto-repairable findings.

Why: combining inspection and mutation would make CI use unsafe and would blur the audit trail. Separate commands preserve a clean dry-run/apply distinction.

Alternative considered: a single self-healing command. Rejected because it hides mutations behind inspection and makes non-interactive use harder to trust.

### Decision 3: The first repair slice only auto-repairs deterministic local surfaces

Auto-repair in `repair-loop-v1` will be limited to surfaces whose producer path already exists locally and deterministically, such as storage-maintenance actions, structural refreshes, and future export refresh hooks that declare a producer path. Trust conflicts, stale rationale conflicts that require human judgment, and absent overlay surfaces remain report-only findings.

Why: this preserves the graph-versus-overlay trust boundary and avoids inventing speculative repairs for surfaces that still need human review.

Alternative considered: allowing `sync` to clear or rewrite any stale surface uniformly. Rejected because some surfaces are advisory or review-driven by design.

### Decision 4: Unsupported and absent surfaces are explicit finding states

When a surface is named in the repair contract but is not yet implemented or not materialized in the current repo, `check` reports it as unsupported or not applicable instead of silently skipping it.

Why: silence would make the repair loop look complete when it is only partially available. Explicit unsupported findings keep the contract honest while the product grows.

Alternative considered: omitting missing surfaces from reports entirely. Rejected because it hides capability gaps and makes CI interpretation ambiguous.

### Decision 5: Resolution logging is append-only and stored under `.synrepo/state/`

Each mutating `sync` run will append a structured record containing timestamp, source revision, requested scope, findings considered, actions taken, and final outcome. The design assumes a path like `.synrepo/state/repair-log.jsonl`.

Why: repair needs auditability across repeated runs, and append-only JSONL is easy to inspect, diff, and collect in CI artifacts.

Alternative considered: rewriting a single last-sync snapshot. Rejected because it loses history and weakens post-failure diagnosis.

### Decision 6: Repair reuses existing maintenance and reconcile primitives

The implementation will compose `collect_diagnostics`, `plan_maintenance` / `execute_maintenance`, and `run_reconcile_pass()` under a new repair-planning layer instead of re-implementing freshness checks and write flows.

Why: these modules already own the current runtime truth for health, compatibility, and structural refresh. The repair loop should orchestrate them, not fork their behavior.

Alternative considered: implementing a completely separate repair executor. Rejected because it would duplicate lock handling, compatibility policy, and runtime-state reporting.

## Risks / Trade-offs

- [Surface coverage outruns implementation] -> Keep unsupported and not-applicable findings explicit so the first slice does not pretend to repair overlay or export systems that do not exist yet.
- [Repair plans become too broad] -> Restrict auto-repair to deterministic local producers and require explicit scope or stable default rules for mutating actions.
- [Resolution log grows indefinitely] -> Store logs in append-only JSONL under `.synrepo/state/` and defer retention policy to a later maintenance slice.
- [Pattern-surface sequencing drift] -> Keep broken declared-link and stale-rationale checks defined in terms of human-authored rationale surfaces, but implement them only against the outputs that `pattern-surface-v1` establishes.

## Migration Plan

No external migration is expected. This change should layer onto the existing CLI and `.synrepo/state/` layout:

1. add repair finding and logging structures,
2. wire `synrepo check` as a read-only surface,
3. wire `synrepo sync` to existing maintenance and reconcile paths,
4. update docs and skill guidance once the commands are real.

Rollback is straightforward: remove the new CLI commands and repair-state artifacts without changing the canonical graph schema.

## Open Questions

- Should `synrepo check` exit non-zero only for actionable and blocked findings, or also for unsupported-surface findings in strict CI mode?
- Should `synrepo sync` default to all auto-repairable findings, or require explicit scope selection once multiple repairable surfaces exist?
