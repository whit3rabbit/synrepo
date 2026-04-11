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
synrepo SHALL define retention and maintenance behavior for runtime stores by consuming the storage-compatibility contract, so maintenance behavior is predictable under upgrades and long-lived usage.

#### Scenario: Maintain runtime stores over time
- **WHEN** a user upgrades synrepo or storage exceeds the declared lifecycle boundaries
- **THEN** the ops contract applies the storage-defined store classes and compatibility actions to determine which stores are compacted, migrated, rebuilt, retained, invalidated, or garbage-collected
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

### Requirement: Sequence watch and reconcile after structural graph production exists
synrepo SHALL treat watch and reconcile behavior as downstream of the structural compile that populates the graph automatically, so watcher-driven updates rerun a deterministic producer path instead of inventing a second truth source.

#### Scenario: Plan the first watch-enabled runtime
- **WHEN** a contributor prepares to implement watch and reconcile behavior
- **THEN** the contract assumes an existing structural compile that already rebuilds the relevant graph and substrate state deterministically
- **AND** watcher behavior is defined as a trigger and coalescing layer over that compile path rather than as a separate source of graph facts

### Requirement: Define the first watch-triggered reconcile loop
synrepo SHALL define a watch-triggered update loop that coalesces local filesystem churn into bounded refresh work and uses reconcile passes to correct missed or ambiguous watcher events.

#### Scenario: Handle a burst of local repository changes
- **WHEN** many filesystem events arrive during a build, refactor, or branch switch
- **THEN** synrepo coalesces them into a bounded update cycle instead of naively running one compile per event
- **AND** a defined reconcile path can restore correctness if watcher coverage is incomplete or stale

### Requirement: Define initial single-writer runtime safety
synrepo SHALL define the initial single-writer safety model for standalone CLI and future daemon-assisted operation, including lock acquisition, conflict behavior, and recovery expectations.

#### Scenario: Two control surfaces could write runtime state
- **WHEN** multiple local control surfaces attempt to mutate `.synrepo/` state concurrently
- **THEN** the contract defines which writer proceeds, which writer waits or fails, and how the resulting state remains consistent
- **AND** correctness does not depend on undocumented process timing

### Requirement: Expose initial reconcile and runtime diagnostics
synrepo SHALL expose a small operational diagnostics surface sufficient to understand stale state, recent reconcile outcomes, lock conflicts, and maintenance needs for the current runtime.

#### Scenario: Investigate why synrepo appears stale
- **WHEN** a user or agent needs to understand whether watch and reconcile behavior is healthy
- **THEN** synrepo provides observable diagnostics about reconcile health, writer ownership, or stale runtime state
- **AND** the operator does not have to infer system state from silent background behavior
