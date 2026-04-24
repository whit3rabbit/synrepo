## 1. Trust View Model

- [ ] 1.1 Define a dashboard trust view model sourced from the shared status snapshot and bounded recent activity.
- [ ] 1.2 Add context rows for cards served, average tokens, tokens avoided, stale responses, and truncation or escalation counts.
- [ ] 1.3 Add overlay-note rows for active, stale, unverified, superseded, forgotten, and invalid counts.

## 2. Current Change Impact

- [ ] 2.1 Add a bounded current-change summary for changed files, affected symbols, linked tests, and open risks when data is available.
- [ ] 2.2 Label degraded or unavailable data instead of omitting it silently.
- [ ] 2.3 Ensure all samples are capped and read-only.

## 3. Dashboard Rendering

- [ ] 3.1 Add the Trust view or tab to the TUI without removing existing Health behavior.
- [ ] 3.2 Add no-data rendering for repos with no context metrics yet.
- [ ] 3.3 Add snapshot or view-model tests for healthy, stale, and degraded trust states.

## 4. Verification

- [ ] 4.1 Run focused dashboard/view-model tests.
- [ ] 4.2 Run `cargo test` for affected dashboard, status, and overlay-note surfaces.
- [ ] 4.3 Run `openspec validate trust-dashboard-v2`.
- [ ] 4.4 Run `openspec status --change trust-dashboard-v2 --json` and confirm `isComplete: true`.
