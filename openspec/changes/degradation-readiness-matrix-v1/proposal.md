## Why

Synrepo already has graceful-degradation behavior in several subsystems, but the readiness states are spread across status, probe, parser, git, watch, and overlay surfaces. This change defines one visible matrix so partial capability is explicit and actionable.

## What Changes

- Define readiness states and next actions for parser failures, missing git, disabled or missing embeddings, disabled watch, stale index, overlay unavailable, and compatibility blocks.
- Surface those states through runtime probe, status, doctor, and dashboard consumers without duplicating scan logic.
- Require degraded cards or workflow outputs to label missing data sources instead of silently returning partial truth.
- Preserve existing behavior where a feature is optional; unavailable is not always blocked.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `runtime-probe`: adds structured capability readiness categories.
- `dashboard`: renders readiness matrix states.
- `bootstrap`: reports post-init degraded capabilities and next actions.
- `structural-parse`: parser failures map into readiness states.
- `git-intelligence`: missing or degraded git maps into readiness states.
- `watch-and-ops`: disabled or unavailable watch maps into readiness states.

## Impact

- Shared status/probe structs and renderers.
- Dashboard and doctor output.
- Existing parser, git, watch, embedding, overlay, and compatibility diagnostics.
- No new daemon, parser, or storage truth source.
