## 1. Trust View Model

- [x] 1.1 Define a dashboard trust view model sourced from the shared status snapshot and bounded recent activity.
- [x] 1.2 Add context rows for cards served, average tokens, tokens avoided, stale responses, and truncation or escalation counts.
- [x] 1.3 Add overlay-note rows for active, stale, unverified, superseded, forgotten, and invalid counts.

## 2. Current Change Impact

- [x] 2.1 Add a bounded current-change summary for changed files, affected symbols, linked tests, and open risks when data is available.
- [x] 2.2 Label degraded or unavailable data instead of omitting it silently.
- [x] 2.3 Ensure all samples are capped and read-only.

## 3. Dashboard Rendering

- [x] 3.1 Add the Trust view or tab to the TUI without removing existing Health behavior.
- [x] 3.2 Add no-data rendering for repos with no context metrics yet.
- [x] 3.3 Add snapshot or view-model tests for healthy, stale, and degraded trust states.

## 4. Verification

- [x] 4.1 Run focused dashboard/view-model tests.
- [x] 4.2 Run `cargo test` for affected dashboard, status, and overlay-note surfaces.
- [x] 4.3 Run `openspec validate trust-dashboard-v2`.
- [x] 4.4 Run `openspec status --change trust-dashboard-v2 --json` and confirm `isComplete: true`.
