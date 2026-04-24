## ADDED Requirements

### Requirement: Expose trust-ready context metric aggregates
Context accounting SHALL provide aggregate fields suitable for dashboard trust rendering without requiring the dashboard to parse individual card responses.

#### Scenario: Dashboard consumes context metrics
- **WHEN** the dashboard requests context trust data from the shared status snapshot
- **THEN** the snapshot exposes cards served, average card tokens, estimated tokens avoided, stale responses, truncation counts, and escalation counts when metrics exist
- **AND** the dashboard does not read `.synrepo/state/context-metrics.json` through a second ad hoc path

#### Scenario: Context metrics are absent
- **WHEN** no context metrics file or counters exist
- **THEN** the snapshot reports the metric group as absent
- **AND** renderers can distinguish absent metrics from zero-value metrics
