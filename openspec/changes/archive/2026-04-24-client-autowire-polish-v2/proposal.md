## Why

`operator-surface-v1` adds multi-client setup flags, but teams still need a clearer setup report: what was detected, what was changed, what was skipped, and which generated shims are stale. This change polishes client auto-wiring without changing the core setup contract.

## What Changes

- Add a detected-client summary to setup and agent-setup flows.
- Add a per-client action report that distinguishes written, registered, current, skipped, unsupported, and failed outcomes.
- Define project/global mode reporting so users understand where config was written.
- Define shim freshness checks and `--regen` guidance without silent overwrites.
- Preserve existing single-client positional behavior and `--only` / `--skip` semantics from `operator-surface-v1`.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `bootstrap`: setup and agent-setup flows gain richer detection and outcome reporting.
- `agent-doctrine`: generated shim freshness and doctrine pointer behavior become visible in setup reports.

## Impact

- Setup and agent-setup renderers.
- Agent target detection summaries.
- Shim freshness checks and tests.
- No new supported client list unless implementation discovers an already-supported target missing from reporting.
