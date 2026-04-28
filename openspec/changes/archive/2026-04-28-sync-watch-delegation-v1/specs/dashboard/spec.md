## ADDED Requirements

### Requirement: Dashboard binds reconcile, sync, and auto-sync keys
The dashboard SHALL bind uppercase `R` to reconcile-now, uppercase `S` to sync-now, and uppercase `A` to auto-sync toggle. Lowercase `r` SHALL continue to refresh the status snapshot only. In live mode these keys SHALL delegate via the watch control socket; in poll mode `R` and `S` SHALL spawn a detached worker thread that takes the writer lock and runs the operation locally, while `A` SHALL show a toast indicating that auto-sync requires an active watch service.

#### Scenario: User presses R in live mode
- **WHEN** the dashboard is running with a live watch service and the user presses `R`
- **THEN** the dashboard sends `reconcile_now` over the control socket
- **AND** surfaces the returned outcome in the log pane

#### Scenario: User presses S in poll mode
- **WHEN** the dashboard is running without a watch service and the user presses `S`
- **THEN** the dashboard spawns a detached thread that runs `execute_sync` locally
- **AND** the existing activity spinner lights while the thread runs
- **AND** a completion toast displays the repaired/blocked counts

#### Scenario: User presses A
- **WHEN** the dashboard is running with a live watch service and the user presses `A`
- **THEN** the dashboard sends `set_auto_sync { enabled: !current }` and updates the header indicator on ack

### Requirement: Dashboard header shows auto-sync state
The dashboard header SHALL render the current auto-sync state as `auto-sync:on` or `auto-sync:off` next to the watch indicator when watch is active, and SHALL render `auto-sync:inactive` (or omit the segment) when watch is not active.

#### Scenario: Header renders live state
- **WHEN** the dashboard is attached to a live watch service with `auto_sync_enabled = true`
- **THEN** the header includes `auto-sync:on`

#### Scenario: Header reflects a runtime toggle
- **WHEN** the user presses `A` and the service acks
- **THEN** the header's auto-sync segment flips to `auto-sync:off` on the next frame

### Requirement: Stale-surface next-action hints reference the new bindings
The dashboard's `NextAction` rendering SHALL, when a stale surface is present that `R` or `S` can repair, include a short `(press R)` or `(press S)` hint next to the action label so users discover the bindings.

#### Scenario: Stale reconcile with fresh bindings
- **WHEN** the snapshot reports reconcile is stale and the dashboard has an `R` binding
- **THEN** the rendered next-action line includes `(press R)` next to the reconcile suggestion
