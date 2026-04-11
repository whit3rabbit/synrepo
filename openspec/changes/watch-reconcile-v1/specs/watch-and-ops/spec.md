## ADDED Requirements

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
