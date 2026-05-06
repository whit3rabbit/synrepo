## Purpose
Define first-run initialization, generated assistant-facing setup, and post-init health behavior for synrepo bootstrap flows.
## Requirements
### Requirement: Define first-run initialization flow
synrepo SHALL define a first-run bootstrap flow that initializes project state, inspects the repository, selects the appropriate mode, and guides the user to the first useful output.

#### Scenario: Run guided setup on a fresh clone
- **WHEN** a user runs bare `synrepo` or interactive no-flag `synrepo init` in a repository with no prior setup
- **THEN** synrepo opens the guided setup flow that initializes runtime state, offers agent integration, offers optional embeddings and explain configuration, and lands in the dashboard
- **AND** the flow does not require manual authoring or a second command before structural value appears

#### Scenario: Run runtime-only init on a fresh clone
- **WHEN** a user runs `synrepo init --mode auto`, `synrepo init --mode curated`, `synrepo init --gitignore`, or non-TTY `synrepo init`
- **THEN** synrepo runs the low-level bootstrap path without entering the setup wizard
- **AND** scripted callers keep deterministic init behavior

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
synrepo SHALL define whether bootstrap is one-shot, re-runnable, or partially recoverable, including how existing `.synrepo/` state and degraded setup outcomes are reported. When a user invokes the smart entry experience (bare `synrepo`) on a partial install, the routing contract SHALL direct them to the repair path defined in the runtime-probe contract rather than to first-run initialization. Interactive no-flag `synrepo init` SHALL route to guided setup only for a fresh uninitialized repository; ready, partial, flagged, and non-TTY init invocations SHALL continue to honor low-level init idempotence semantics.

#### Scenario: Re-run init in an already initialized repository
- **WHEN** a user runs `synrepo init` after `.synrepo/` already exists or a prior bootstrap only partially completed
- **THEN** the contract defines whether synrepo refuses, repairs, refreshes, or redirects to another command
- **AND** the result includes a clear health or failure state rather than ambiguous partial setup

#### Scenario: No-flag init on a fresh TTY repo
- **WHEN** a user runs `synrepo init` with no flags in an uninitialized repository and stdout is a TTY
- **THEN** synrepo opens the guided setup wizard instead of stopping after runtime bootstrap
- **AND** selecting explain in that wizard writes the same `[explain]` config as the explain-only setup path
- **AND** selecting embeddings in that wizard writes `enable_semantic_triage = true` before the initial runtime build

#### Scenario: Bare entry on a partial install
- **WHEN** a user runs bare `synrepo` in a repository whose runtime probe returns `partial`
- **THEN** the binary routes to the guided repair experience and preserves existing `.synrepo/` state
- **AND** the user is not prompted to start a new project or to re-run first-run initialization

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
synrepo SHALL support `cursor`, `codex`, and `windsurf` as named targets for `synrepo agent-setup`, in addition to the existing `claude`, `copilot`, and `generic` targets. A `--regen` flag SHALL update an existing shim file in place when its content differs from the current template. The `synrepo setup` and `synrepo agent-setup` commands SHALL accept `--only <tool,tool>` and `--skip <tool,tool>` for multi-client invocation, and SHALL reject the combination of both flags as a usage error.

#### Scenario: Generate a cursor shim
- **WHEN** a user runs `synrepo agent-setup cursor`
- **THEN** synrepo writes a shim to `.cursor/skills/synrepo/SKILL.md` describing the available MCP tools and their usage
- **AND** the shim begins with YAML frontmatter containing `name: synrepo` and a `description` so Cursor auto-discovers it as a skill
- **AND** the shim content reflects the current shipped MCP surface

#### Scenario: Regenerate an existing shim
- **WHEN** a user runs `synrepo agent-setup claude --regen` and the existing shim differs from the current template
- **THEN** synrepo overwrites the shim and prints a summary of what changed
- **AND** if the shim is already current, the command exits zero with no changes

#### Scenario: Generate a codex shim
- **WHEN** a user runs `synrepo agent-setup codex`
- **THEN** synrepo writes a shim to `.agents/skills/synrepo/SKILL.md` describing the MCP server and tool list
- **AND** the shim begins with YAML frontmatter containing `name: synrepo` and a `description` so Codex CLI auto-discovers it as a skill
- **AND** the shim notes that Codex MCP uses `~/.codex/config.toml` or trusted project `.codex/config.toml` with `[mcp_servers.synrepo]`

#### Scenario: Multi-client setup with --only
- **WHEN** a user runs `synrepo setup --only claude,cursor`
- **THEN** synrepo configures both clients in sequence and prints a per-tool outcome summary
- **AND** a single-tool positional invocation (`synrepo setup claude`) continues to work unchanged

#### Scenario: Multi-client setup with --skip
- **WHEN** a user runs `synrepo agent-setup --skip copilot,generic`
- **THEN** synrepo configures every other supported tool for which the host has detection signals
- **AND** the per-tool summary names every skipped tool

#### Scenario: Conflicting flags rejected
- **WHEN** a user runs `synrepo setup --only claude --skip claude`
- **THEN** synrepo rejects the invocation with a usage error naming the conflict
- **AND** no shim or MCP registration is written

#### Scenario: Unknown tool rejected
- **WHEN** a user runs `synrepo setup --only claude,nonesuch`
- **THEN** synrepo rejects the invocation with an error naming `nonesuch` and listing supported tools
- **AND** no partial configuration is left on disk

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

### Requirement: Report degraded capabilities after bootstrap
Bootstrap success and repair output SHALL report degraded optional and core capabilities using the same readiness labels as runtime probe.

#### Scenario: Bootstrap completes with degraded optional capability
- **WHEN** bootstrap succeeds but git history or embeddings are unavailable
- **THEN** the success output reports the capability as unavailable or disabled with the relevant next action
- **AND** core graph readiness remains successful when source-derived graph operation is usable

#### Scenario: Bootstrap completes with partial core capability
- **WHEN** bootstrap completes but parser failures or stale index state limit graph coverage
- **THEN** the output reports the degraded capability and next action
- **AND** it does not claim full readiness for the affected surface

### Requirement: Report per-client setup outcomes
`synrepo setup` and `synrepo agent-setup` SHALL report a per-client outcome summary for resolved agent targets.

#### Scenario: Multi-client setup completes
- **WHEN** a user runs setup or agent-setup for multiple targets
- **THEN** the output lists each resolved target with an outcome such as written, registered, current, skipped, unsupported, stale, or failed
- **AND** the output includes the relevant project or global config path when a path is known

#### Scenario: Single-client behavior is used
- **WHEN** a user runs an existing positional invocation such as `synrepo setup claude`
- **THEN** the command preserves existing behavior
- **AND** it may add the per-client outcome summary without changing which files are written

### Requirement: Distinguish detection from mutation
Client detection during setup SHALL be observational until the existing setup confirmation or command execution path performs writes.

#### Scenario: Clients are detected
- **WHEN** setup detects one or more supported clients on the host
- **THEN** the output labels them as detected candidates
- **AND** no shim or MCP config is written solely because detection occurred

### Requirement: Report shim freshness without silent overwrite
Setup reporting SHALL distinguish current, missing, and stale generated shims, and SHALL NOT overwrite stale shims unless the existing `--regen` policy allows it.

#### Scenario: Generated shim is stale
- **WHEN** a generated shim differs from the current template and the user did not request regeneration
- **THEN** setup reports the shim as stale and names the regeneration action
- **AND** the existing shim content is not overwritten silently

### Requirement: Register projects during setup
`synrepo setup <tool>` and `synrepo setup <tool> --global` SHALL ensure the current repository is recorded in the user-level project registry after initialization succeeds. Setup SHALL preserve existing registry metadata and SHALL NOT record a project whose initialization or readiness step fails.

#### Scenario: Scripted setup registers project
- **WHEN** the user runs `synrepo setup claude` in a repository that initializes successfully
- **THEN** synrepo records the repository in `~/.synrepo/projects.toml`
- **AND** repeated setup does not create duplicate project entries

#### Scenario: Setup readiness fails
- **WHEN** setup cannot initialize or prepare the repository for normal operation
- **THEN** synrepo reports the setup failure
- **AND** it does not add a new managed project entry for the failed repository

### Requirement: Configure global MCP entries without a repo flag
When setup performs MCP registration for a supported agent target, it SHALL default to a user-scoped (global) agent configuration that launches `synrepo mcp` without `--repo .` whenever the target supports `Scope::Global` (as reported by the underlying installer). Project-scoped setup SHALL remain available via an explicit `--project` opt-in and SHALL write a repository-scoped entry that launches `synrepo mcp --repo .`. The setup flow SHALL persist the chosen scope so re-running setup is idempotent in either mode.

#### Scenario: Default global setup for a supported target
- **WHEN** the user runs `synrepo setup claude`
- **THEN** synrepo writes or updates the user-scoped Claude MCP config
- **AND** the `synrepo` server command launches `synrepo mcp`
- **AND** the current repository is registered as a managed project

#### Scenario: Explicit project-scoped setup
- **WHEN** the user runs `synrepo setup claude --project`
- **THEN** synrepo writes or updates the project-scoped MCP config
- **AND** the `synrepo` server command launches `synrepo mcp --repo .`

#### Scenario: Re-run yields no diff when scope is unchanged
- **WHEN** the user re-runs `synrepo setup claude` with the same scope as a prior install
- **THEN** synrepo reports the install as already current
- **AND** no file content changes on disk

### Requirement: Report unsupported global targets clearly
If an agent target is not supported by the installer at `Scope::Global`, `synrepo setup <tool>` SHALL detect this before any write and either fall back to project-scoped registration with an explicit notice, or refuse with a clear message that names the unsupported target. It SHALL NOT silently write a project-scoped MCP entry while claiming global setup.

#### Scenario: Global setup target lacks writer
- **WHEN** the user runs `synrepo setup <tool>` for a target without supported global MCP registration
- **THEN** synrepo reports that global MCP registration is unsupported for that target
- **AND** the operator is shown how to opt into project-scoped setup with `--project`
- **AND** no project-scoped MCP config is written without that explicit opt-in

#### Scenario: Multi-client default-global setup has mixed support
- **WHEN** the user runs `synrepo setup --only claude,codex` (default global)
- **THEN** synrepo reports per-client outcomes
- **AND** targets with global support are configured globally while targets without it are reported as unsupported or project-scoped per the operator's explicit choice

### Requirement: Delegate agent integration writes to the agent-config installer
synrepo SHALL delegate MCP server registration, agent skill placement, and agent instruction placement to the `agent-config` crate's installer surface (`McpSpec`/`SkillSpec`/`InstructionSpec` plus the `mcp_by_id`/`skill_by_id`/`instruction_by_id` registries). The installer SHALL be invoked with `owner = "synrepo"` so subsequent `synrepo remove` operations are scoped by ownership tag and cannot disturb other consumers' entries. The installer's atomicity guarantees (write-to-temp-and-rename, first-touch `.bak` backup, idempotent re-install, ownership ledger) SHALL be preserved at the synrepo boundary; synrepo SHALL NOT bypass the installer with hand-rolled JSON or TOML edits for the same surfaces.

#### Scenario: Setup writes through the installer
- **WHEN** the user runs `synrepo setup` for any supported target
- **THEN** the resulting MCP, skill, or instruction file changes are produced by the agent-config installer
- **AND** any pre-existing target file the installer modified has a single `<path>.bak` sibling created on first touch

#### Scenario: Removal is owner-scoped
- **WHEN** the user runs `synrepo remove <tool>` for a target previously installed by `synrepo setup`
- **THEN** synrepo invokes the installer's uninstall path keyed on `(name = "synrepo", owner = "synrepo")`
- **AND** unrelated entries belonging to other owners or other server names are preserved

#### Scenario: Re-install after a change to spec content updates the file
- **WHEN** the user re-runs setup after the doctrine or shim content changes
- **THEN** the installer reports the file as patched (not already-installed)
- **AND** the on-disk file matches the new spec content byte-for-byte

### Requirement: Surface installer-reported file paths in setup output
`synrepo setup` SHALL surface the absolute path of every file created or patched during the run, sourced from the installer's report rather than from a synrepo-side hard-coded table. Output SHALL distinguish created from patched targets and SHALL identify each by display name.

#### Scenario: Setup prints created and patched paths
- **WHEN** `synrepo setup claude` results in a created MCP entry and a patched skill file
- **THEN** the output names both files by their absolute path
- **AND** the labels distinguish "created" vs "patched"

### Requirement: Migrate pre-existing installs without ownership markers
`synrepo upgrade --apply` SHALL detect MCP, skill, or instruction targets that were written by an earlier synrepo version (no `_agent_config_tag` marker, no ownership ledger entry) and offer to adopt them by replaying the install through the agent-config installer with `owner = "synrepo"`. Adoption SHALL be idempotent and SHALL refuse to clobber a target whose content does not match the current synrepo spec without an explicit confirmation step.

#### Scenario: Legacy install adopted on upgrade
- **WHEN** the user runs `synrepo upgrade --apply` against a repository whose `.mcp.json` contains a `synrepo` entry written before this change
- **THEN** the upgrade replays the install through the installer, adding the `_agent_config_tag` marker and the ownership ledger entry
- **AND** subsequent `synrepo remove` succeeds without manual editing

#### Scenario: Legacy content differs from current spec
- **WHEN** the legacy entry's content does not match what the current synrepo would write
- **THEN** the upgrade reports the divergence and exits non-zero unless the operator passes a confirmation flag
- **AND** no file is mutated without the confirmation

### Requirement: Guide full product uninstall
`synrepo uninstall` SHALL provide a guided full-product teardown distinct from repo-local `synrepo remove`. The flow SHALL plan synrepo-owned agent skills or instructions, MCP entries, Git hooks, project `.synrepo/` directories, generated export output, `~/.synrepo`, and binary removal or package-manager follow-up. Database/cache data rows SHALL be kept by default unless the operator selects them in the TTY wizard or passes `--delete-data` in a non-interactive run.

#### Scenario: Dry-run keeps data by default
- **WHEN** the user runs `synrepo uninstall --json`
- **THEN** the plan includes project `.synrepo/` and `~/.synrepo` rows when present
- **AND** those rows are marked disabled by default
- **AND** agent/MCP/hook rows are marked enabled when they are synrepo-owned

#### Scenario: Full data teardown is explicit
- **WHEN** the user runs `synrepo uninstall --apply --force --delete-data`
- **THEN** project `.synrepo/` directories and generated export directories are selected
- **AND** root `.gitignore` cleanup for `.synrepo/` or export output runs only after the corresponding generated directory deletion succeeds

#### Scenario: Binary removal is last and guarded
- **WHEN** the current executable is a direct install path outside the repository build output
- **THEN** `synrepo uninstall` plans direct binary deletion after project and global cleanup
- **AND** Homebrew or Cargo-managed binaries are reported as manual follow-up commands instead of being deleted directly
