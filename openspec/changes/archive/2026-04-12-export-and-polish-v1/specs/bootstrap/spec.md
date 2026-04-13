## ADDED Requirements

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
