## Purpose
Define first-run initialization, generated assistant-facing setup, and post-init health behavior for synrepo bootstrap flows.

## Requirements

### Requirement: Define first-run initialization flow
synrepo SHALL define a first-run bootstrap flow that initializes project state, inspects the repository, selects the appropriate mode, and guides the user to the first useful output.

#### Scenario: Run `synrepo init` on a fresh clone
- **WHEN** a user initializes synrepo in a repository with no prior setup
- **THEN** the bootstrap contract defines the setup steps and expected first-run outputs
- **AND** the flow does not require manual authoring before structural value appears

### Requirement: Define mode selection semantics
synrepo SHALL define how auto mode and curated mode are selected, overridden, and revisited based on repository signals and explicit user intent.

#### Scenario: Detect rationale sources during bootstrap
- **WHEN** bootstrap inspects a repository that contains ADRs, concept directories, or similar rationale material
- **THEN** the contract defines whether synrepo selects curated mode automatically, recommends it, or preserves a prior explicit choice
- **AND** later repository changes can trigger a defined refresh or review path instead of silent mode drift

### Requirement: Define generated assistant-facing setup
synrepo SHALL define when generated shims, thin instructions, or assistant-facing setup files may be created during bootstrap and how they relate to existing repository guidance.

#### Scenario: Configure an agent-facing project shim
- **WHEN** bootstrap decides to generate an assistant-facing artifact
- **THEN** the bootstrap spec defines its purpose as a thin convenience surface
- **AND** it does not replace the canonical planning or runtime layers

### Requirement: Define post-init health checks
synrepo SHALL define post-initialization health checks and refresh behavior so the user can understand whether the project is ready for normal operation.

#### Scenario: Review project health after initialization
- **WHEN** a bootstrap flow completes
- **THEN** the user receives a defined health summary and recommended next steps
- **AND** the contract identifies when follow-up refresh or repair is required

### Requirement: Define init idempotence and failure states
synrepo SHALL define whether bootstrap is one-shot, re-runnable, or partially recoverable, including how existing `.synrepo/` state and degraded setup outcomes are reported.

#### Scenario: Re-run init in an already initialized repository
- **WHEN** a user runs `synrepo init` after `.synrepo/` already exists or a prior bootstrap only partially completed
- **THEN** the contract defines whether synrepo refuses, repairs, refreshes, or redirects to another command
- **AND** the result includes a clear health or failure state rather than ambiguous partial setup

### Requirement: Define mandatory first-run outputs
synrepo SHALL define the minimum first-run outputs that every successful bootstrap must provide, including mode, health state, and next-step guidance.

#### Scenario: Complete bootstrap successfully
- **WHEN** `synrepo init` succeeds
- **THEN** the user receives the mandatory first-run summary required by the bootstrap contract
- **AND** optional generated shims or guidance remain additive rather than substituting for the required output

### Requirement: Trigger structural graph population during bootstrap
synrepo SHALL run the deterministic structural compile during successful bootstrap and refresh flows after the lexical substrate has been rebuilt, so first-run initialization also materializes the current observed-facts graph.

#### Scenario: Complete bootstrap on a repository with supported inputs
- **WHEN** `synrepo init` succeeds in a repository containing supported code or configured concept markdown
- **THEN** bootstrap triggers the structural compile after rebuilding the lexical substrate
- **AND** the resulting runtime state includes a materialized graph store that reflects current repository inputs

### Requirement: Report graph population status in bootstrap output
synrepo SHALL include graph-oriented status in the bootstrap summary when structural graph population runs, including whether the graph was built or refreshed and whether the runtime is ready for graph inspection commands.

#### Scenario: Review bootstrap output after a graph-producing init
- **WHEN** a bootstrap flow completes after running the structural compile
- **THEN** the user receives status text that distinguishes lexical-index work from graph-population work
- **AND** the next-step guidance remains clear about what graph-oriented commands are now usable
