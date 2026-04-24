## ADDED Requirements

### Requirement: Provide a trust-focused dashboard view
The dashboard SHALL provide a trust-focused view that reports context quality, advisory overlay health, degraded surfaces, and bounded current-change impact without duplicating status or repair scan logic.

#### Scenario: Dashboard renders trust signals
- **WHEN** the dashboard opens on a ready repository with context metrics or overlay-note data
- **THEN** it exposes cards served, average card tokens, estimated tokens avoided, stale responses, truncation or escalation counts, and overlay-note lifecycle counts
- **AND** each row is sourced from the shared status snapshot, context metrics, repair report, recent activity, or overlay-note aggregate data

#### Scenario: Trust data has not been recorded
- **WHEN** no context metrics or overlay-note aggregates exist yet
- **THEN** the trust view labels the relevant group as no data
- **AND** it does not render no-data as a proven zero-count healthy state

### Requirement: Surface bounded current-change impact
The dashboard SHALL provide a bounded current-change impact summary when changed-file, symbol, test, or risk data is available.

#### Scenario: Current change data is available
- **WHEN** the status snapshot or bounded query layer can identify changed files with affected symbols, linked tests, or open risks
- **THEN** the trust view displays a capped summary of those items
- **AND** the summary labels unavailable data sources instead of silently omitting them
