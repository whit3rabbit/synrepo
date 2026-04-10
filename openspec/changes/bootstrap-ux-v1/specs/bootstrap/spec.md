## MODIFIED Requirements

### Requirement: Define first-run initialization flow
synrepo SHALL define `synrepo init` as a first-run flow that creates the runtime workspace, inspects the repository, chooses or confirms an operating mode, and reports substrate setup progress.

#### Scenario: Initialize a clean repository
- **WHEN** a user runs `synrepo init` in a repository with no existing `.synrepo/`
- **THEN** synrepo creates the declared runtime layout and bootstraps the initial substrate state
- **AND** the command reports the mode and setup progress as part of first-run output

### Requirement: Define post-init health checks
synrepo SHALL end bootstrap in a declared health state and report enough detail for the user to understand whether the repository is ready, degraded, or blocked.

#### Scenario: Finish init with usable but degraded state
- **WHEN** bootstrap completes with partial capability or a recoverable warning
- **THEN** the command reports a degraded health state instead of claiming fully healthy success
- **AND** the output tells the user what to do next

### Requirement: Define init idempotence and failure states
synrepo SHALL define how `synrepo init` behaves when `.synrepo/` already exists or prior bootstrap state is only partially usable.

#### Scenario: Re-run init after prior setup
- **WHEN** a user runs `synrepo init` in a repository that already contains `.synrepo/`
- **THEN** synrepo follows the declared re-entry behavior rather than failing ambiguously
- **AND** the command tells the user whether to stop, repair, or use a different workflow

## ADDED Requirements

### Requirement: Define mode selection precedence
synrepo SHALL define precedence between explicit mode flags, repository-detected rationale signals, and bootstrap defaults.

#### Scenario: User requests auto mode in a repo with ADR directories
- **WHEN** the repository contains rationale sources that suggest curated mode but the user passes `--mode auto`
- **THEN** synrepo honors the explicit mode choice
- **AND** any curated-mode recommendation is surfaced as guidance rather than as a silent override

### Requirement: Define mandatory first-run summary fields
synrepo SHALL emit a minimum first-run summary after successful bootstrap that includes the chosen mode, runtime location, substrate status, and next-step guidance.

#### Scenario: Complete init successfully
- **WHEN** bootstrap finishes in a healthy state
- **THEN** the CLI output includes the mandatory summary fields defined by the bootstrap contract
- **AND** optional extra guidance remains additive rather than required for basic usability
