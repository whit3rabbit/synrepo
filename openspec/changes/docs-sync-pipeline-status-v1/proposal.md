## Why

The module docstring at `src/pipeline/structural/mod.rs:14-17` still claims "Stages 5–8 (git mining, identity cascade, drift scoring, ArcSwap commit) remain TODO stubs." Stages 5, 6, and 7 have shipped (`git-intelligence-v1`, `graph-lifecycle-v1` identity cascade, `structural-resilience-v1` drift scoring). Only stage 8 (ArcSwap commit) is still outstanding. The sibling claim in `src/lib.rs:15` is already accurate — it calls out only stage 8 — so the inconsistency is internal to the codebase and will mislead every reader who walks the structural pipeline from its main entrypoint.

Stale architecture comments are load-bearing here: contributors reading `structural/mod.rs` to understand pipeline scope will assume git intelligence, identity cascade, and drift scoring are future work, and may propose designs that duplicate or conflict with what is already shipped. Fixing the docs now, before the next wave of pipeline work begins, keeps the code its own source of truth.

## What Changes

- Update the module docstring at `src/pipeline/structural/mod.rs:14-17` to describe the current stage status accurately: stages 1–7 are wired; only stage 8 (ArcSwap commit) remains TODO.
- Keep `src/lib.rs:15` — already accurate, do not churn.
- No runtime behaviour change. Docs-only.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

None. This change updates internal module documentation only; no behaviour spec is affected.

## Impact

- **Code**: `src/pipeline/structural/mod.rs` (docstring only).
- **APIs**: None.
- **Dependencies**: None.
- **Systems**: None.
- **Docs**: The in-tree module docstring becomes consistent with `AGENTS.md` / `CLAUDE.md` phase-status section, which already documents stages 5–7 as shipped.
