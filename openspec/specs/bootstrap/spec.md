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

### Requirement: Define upgrade command contract
synrepo SHALL provide a `synrepo upgrade` command that detects `.synrepo/` version skew, determines the required compatibility action for each store, and executes the actions only when the user passes `--apply`. Without `--apply` the command prints a dry-run plan and exits with a non-zero code if any store requires action.

#### Scenario: Run upgrade dry-run after a binary update
- **WHEN** a user runs `synrepo upgrade` after installing a new binary version
- **THEN** synrepo prints a compatibility plan showing each store, its current version, the required action (continue / rebuild / invalidate / migrate), and the expected outcome
- **AND** no stores are mutated until `--apply` is passed

#### Scenario: Run upgrade with apply
- **WHEN** a user runs `synrepo upgrade --apply`
- **THEN** synrepo executes the compatibility actions in the order defined by the compatibility evaluator
- **AND** each store reports its result (continued / rebuilt / invalidated / migrated / blocked)
- **AND** the command exits zero if all stores reach a usable state

#### Scenario: Version skew detected at startup
- **WHEN** synrepo detects that a `.synrepo/` store's recorded version is outside the binary's supported range and the user did not run `upgrade`
- **THEN** synrepo emits a warning recommending `synrepo upgrade` and proceeds with degraded or blocked behavior per the existing compatibility action
- **AND** the warning is not suppressed silently

### Requirement: Define agent-setup target expansion
synrepo SHALL support `cursor`, `codex`, and `windsurf` as named targets for `synrepo agent-setup`, in addition to the existing `claude`, `copilot`, and `generic` targets. A `--regen` flag SHALL update an existing shim file in place when its content differs from the current template.

#### Scenario: Generate a cursor shim
- **WHEN** a user runs `synrepo agent-setup cursor`
- **THEN** synrepo writes a shim to `.cursor/rules/synrepo.mdc` describing the available MCP tools and their usage
- **AND** the shim content reflects the current shipped MCP surface

#### Scenario: Regenerate an existing shim
- **WHEN** a user runs `synrepo agent-setup claude --regen` and the existing shim differs from the current template
- **THEN** synrepo overwrites the shim and prints a summary of what changed
- **AND** if the shim is already current, the command exits zero with no changes

#### Scenario: Generate a codex shim
- **WHEN** a user runs `synrepo agent-setup codex`
- **THEN** synrepo writes a shim to `.codex/instructions.md` describing the MCP server and tool list
- **AND** the shim notes how to configure the MCP server for codex usage

### Requirement: Enrich status output with export and overlay cost summary
synrepo SHALL include export freshness state and overlay cost-to-date in `synrepo status` output so users can assess the health of convenience surfaces and LLM usage without running a full `check`.

#### Scenario: View status with exports present
- **WHEN** a user runs `synrepo status` and `synrepo-context/` contains an export manifest
- **THEN** the status output includes the export freshness state (current / stale / absent) and the manifest timestamp
- **AND** stale exports do not prevent the status command from completing

#### Scenario: View status with overlay usage
- **WHEN** a user runs `synrepo status` and the overlay store contains commentary or cross-link audit rows
- **THEN** the status output includes a cost-to-date summary (total LLM calls and estimated token count from the audit tables)
- **AND** the summary is read-only and does not trigger any generation

### Requirement: First-run report points to the agent doctrine
synrepo SHALL include a single-line pointer to the agent doctrine in the first-run bootstrap success output. The pointer SHALL name the escalation default (tiny → normal → deep) and reference the shim path most recently written by `synrepo agent-setup`, or a generic pointer (for example the `skill/SKILL.md` path or the `agent-setup` command) when no shim has been generated. The full doctrine block SHALL NOT appear in bootstrap output; only the pointer.

#### Scenario: Clean bootstrap with prior agent-setup
- **WHEN** a user runs `synrepo init` on a repository where `synrepo agent-setup <tool>` has already written a shim
- **AND** bootstrap succeeds with clean health
- **THEN** the success output contains the pointer line naming the escalation default and the shim path
- **AND** the full doctrine block does not appear in the report

#### Scenario: Clean bootstrap without prior agent-setup
- **WHEN** a user runs `synrepo init` on a repository with no prior shim
- **AND** bootstrap succeeds
- **THEN** the success output contains a pointer line naming the escalation default and suggesting the user run `synrepo agent-setup <tool>` or read `skill/SKILL.md`
- **AND** the full doctrine block does not appear

#### Scenario: Partial or failed bootstrap
- **WHEN** bootstrap does not reach clean-success health
- **THEN** the pointer line is not included
- **AND** the output focuses on the health issue rather than agent onboarding
