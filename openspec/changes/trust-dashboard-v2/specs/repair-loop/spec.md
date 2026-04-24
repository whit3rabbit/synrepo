## ADDED Requirements

### Requirement: Provide trust-view remediation hints
Repair surfaces SHALL provide enough recommended-action data for dashboard trust rows to tell an operator how to address stale context, stale notes, or degraded advisory surfaces.

#### Scenario: Trust surface is stale
- **WHEN** repair or status classification identifies stale context responses, stale notes, invalid notes, or stale overlay content
- **THEN** the dashboard trust view can display the surface, severity, and recommended action
- **AND** the dashboard does not invent a separate remediation policy
