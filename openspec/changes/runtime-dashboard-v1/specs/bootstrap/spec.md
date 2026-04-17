## ADDED Requirements

### Requirement: Expose a runtime probe alongside bootstrap
synrepo SHALL expose a read-only runtime probe alongside the existing bootstrap flow. The probe SHALL classify a repository's `.synrepo/` state as `uninitialized`, `partial`, or `ready`, and SHALL be callable from the CLI entrypoint without running `bootstrap()` as a side effect.

#### Scenario: Probe without bootstrapping
- **WHEN** the CLI entrypoint runs the runtime probe on an existing repository
- **THEN** the probe returns a classification without triggering `bootstrap()`, acquiring the writer lock, or mutating store contents

### Requirement: Define partial-state routing contract
synrepo SHALL define that a partial `.synrepo/` state routes to a repair path, not to first-run initialization. The bootstrap contract SHALL continue to own init for the uninitialized case, while the repair path SHALL fix missing or blocked components in place.

#### Scenario: Partial install routed to repair
- **WHEN** `.synrepo/` exists but one or more required components are missing, corrupt, or compat-blocked
- **THEN** the routing contract selects the repair path
- **AND** the repair path does not delete or reinitialize existing state without explicit user confirmation

## MODIFIED Requirements

### Requirement: Define init idempotence and failure states
synrepo SHALL define whether bootstrap is one-shot, re-runnable, or partially recoverable, including how existing `.synrepo/` state and degraded setup outcomes are reported. When a user invokes the smart entry experience (bare `synrepo`) on a partial install, the routing contract SHALL direct them to the repair path defined in the runtime-probe contract rather than to first-run initialization, and `synrepo init` itself SHALL continue to honor its existing idempotence semantics when invoked explicitly.

#### Scenario: Re-run init in an already initialized repository
- **WHEN** a user runs `synrepo init` after `.synrepo/` already exists or a prior bootstrap only partially completed
- **THEN** the contract defines whether synrepo refuses, repairs, refreshes, or redirects to another command
- **AND** the result includes a clear health or failure state rather than ambiguous partial setup

#### Scenario: Bare entry on a partial install
- **WHEN** a user runs bare `synrepo` in a repository whose runtime probe returns `partial`
- **THEN** the binary routes to the guided repair experience and preserves existing `.synrepo/` state
- **AND** the user is not prompted to start a new project or to re-run first-run initialization
