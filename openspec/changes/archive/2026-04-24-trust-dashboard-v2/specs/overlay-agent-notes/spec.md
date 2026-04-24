## ADDED Requirements

### Requirement: Surface note lifecycle counts as trust signals
Overlay agent-note lifecycle counts SHALL be available to operator trust views as advisory health signals.

#### Scenario: Notes exist in multiple lifecycle states
- **WHEN** the overlay store contains active, stale, unverified, superseded, forgotten, or invalid notes
- **THEN** the trust view can display counts for each lifecycle state
- **AND** those counts are labeled advisory and overlay-backed

#### Scenario: Stale or invalid notes exist
- **WHEN** stale or invalid note counts are greater than zero
- **THEN** the trust view marks the note surface as degraded
- **AND** the recommended next action points to the appropriate check, sync, verify, or audit surface
