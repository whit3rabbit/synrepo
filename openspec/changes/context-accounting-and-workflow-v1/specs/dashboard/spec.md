## ADDED Requirements

### Requirement: Surface context metrics in operator views
The shared status snapshot SHALL include context metrics so `synrepo status --json` and the dashboard can report card usage and context savings without duplicating logic.

#### Scenario: Dashboard renders context metrics
- **WHEN** the dashboard opens on a ready repository
- **THEN** it can display cards served, average card tokens, estimated raw-file tokens avoided, stale-card counts, and budget tier usage from the shared status snapshot
