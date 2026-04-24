## Why

The dashboard already reports operational readiness, and `operator-surface-v1` adds more operator outputs. The next gap is trust visibility: users need to see whether synrepo is actually serving bounded, fresh, source-labeled context instead of just seeing that the runtime is alive.

## What Changes

- Add a trust-focused dashboard view over context metrics, overlay-note health, stale responses, degraded surfaces, and recent card activity.
- Surface current-change impact signals in a bounded summary: changed files, affected symbols, linked tests, and open risks when available.
- Keep dashboard data sourced from existing status snapshots, context metrics, repair surfaces, recent activity, and overlay-note counts.
- Avoid adding new graph truth or session-memory logging.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `dashboard`: adds a trust view focused on context quality and freshness.
- `context-accounting`: identifies the metrics the dashboard consumes for trust reporting.
- `overlay-agent-notes`: note lifecycle counts become operator-visible trust signals.
- `repair-loop`: stale or degraded trust surfaces provide recommended next actions.

## Impact

- Dashboard view models and TUI rendering.
- Shared status snapshot fields if required by the view.
- Repair/status rows for stale context and overlay-note health.
- No storage migration unless an implementation discovers a missing aggregate already required by existing specs.
