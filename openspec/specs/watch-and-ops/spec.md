## Purpose
Define watcher, reconcile, locking, cache lifecycle, storage compatibility, and diagnostics behavior for operating synrepo under normal repository churn.

## Requirements

### Requirement: Define watcher and reconcile behavior
synrepo SHALL define watch-mode behavior, event coalescing, and reconcile passes so repository churn does not silently poison the system.

#### Scenario: Receive a burst of file events
- **WHEN** many filesystem events arrive during a build or refactor
- **THEN** watch behavior coalesces and reconciles them into bounded structural updates
- **AND** stale or missed events are corrected by a defined reconcile path

### Requirement: Define operational lifecycle boundaries
synrepo SHALL define daemon optionality, locking, cache lifecycle, compact behavior, and failure recovery for local operation.

#### Scenario: Run synrepo with and without a daemon
- **WHEN** a user runs synrepo in standalone or daemon-assisted mode
- **THEN** the ops contract defines the ownership of writes, locks, and snapshots
- **AND** both modes preserve consistent state and recovery expectations

### Requirement: Define store retention and compatibility operations
synrepo SHALL define retention, cleanup, rebuild, and migration operations for runtime stores so maintenance behavior is predictable under upgrades and long-lived usage.

#### Scenario: Maintain runtime stores over time
- **WHEN** a user upgrades synrepo or storage exceeds the declared lifecycle boundaries
- **THEN** the ops contract defines which stores are compacted, migrated, rebuilt, retained, or garbage-collected
- **AND** maintenance behavior is observable instead of implicit

### Requirement: Expose operational diagnostics
synrepo SHALL define operational status and diagnostics surfaces sufficient to understand cache state, reconcile health, and failure recovery needs.

#### Scenario: Investigate operational trouble
- **WHEN** a user or agent needs to understand why synrepo is stale or unhealthy
- **THEN** the ops surface provides observable diagnostics
- **AND** the contract avoids treating watcher behavior as opaque background magic

### Requirement: Define single-writer operational safety
synrepo SHALL define single-writer safety for daemon and standalone operation, including how concurrent writers are prevented or rejected.

#### Scenario: Run multiple control surfaces at once
- **WHEN** an MCP server, CLI, or background process could write runtime state concurrently
- **THEN** the ops contract defines the authoritative writer and locking behavior
- **AND** state consistency does not depend on undocumented process ordering
