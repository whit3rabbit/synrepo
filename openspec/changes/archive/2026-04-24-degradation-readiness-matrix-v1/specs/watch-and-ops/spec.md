## ADDED Requirements

### Requirement: Map watch and daemon state to readiness
Watch and daemon state SHALL appear in the readiness matrix without making the daemon mandatory for normal explicit CLI operation.

#### Scenario: Watch is disabled or stopped
- **WHEN** no watch daemon is active and no foreground watch is running
- **THEN** the readiness matrix marks watch as disabled or stopped
- **AND** the next action names `synrepo watch` or `synrepo watch --daemon` without blocking explicit `sync`, `check`, or card commands

#### Scenario: Watch owner is stale or conflicting
- **WHEN** watch state records a dead owner, live conflicting owner, or unreachable control socket
- **THEN** the readiness matrix marks watch as stale or blocked according to the existing watch-control diagnosis
- **AND** the recommended action follows existing watch cleanup or stop behavior
