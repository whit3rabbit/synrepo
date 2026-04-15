## ADDED Requirements

### Requirement: Define the compact command as an operator surface
synrepo SHALL define `synrepo compact` as an explicit operator command for enforcing retention policies on overlay, state, and index stores. The command SHALL default to dry-run mode and require `--apply` to execute mutations.

#### Scenario: Dry-run compact reports planned actions
- **WHEN** a user runs `synrepo compact` (without `--apply`)
- **THEN** the command prints a summary of each planned action with estimated row counts
- **AND** no stores are mutated
- **AND** the command exits zero

#### Scenario: Apply compact executes planned actions
- **WHEN** a user runs `synrepo compact --apply`
- **THEN** the command executes all planned actions and prints a summary of rows affected per action
- **AND** the command exits zero on success, non-zero on failure

#### Scenario: Apply compact with non-default policy
- **WHEN** a user runs `synrepo compact --apply --policy aggressive`
- **THEN** the compact pass uses the `aggressive` policy retention thresholds instead of `default`

### Requirement: Report compaction status in synrepo status
synrepo SHALL report compaction-relevant counts and timestamps in `synrepo status` output.

#### Scenario: Inspect compactable volume
- **WHEN** a user runs `synrepo status`
- **THEN** the output includes compactable commentary count, compactable cross-link audit count, compactable repair-log entry count, and the timestamp of the last successful compaction (or "never" if not yet run)

#### Scenario: Report last compaction timestamp
- **WHEN** a compact pass completes successfully
- **THEN** the completion timestamp is recorded in `.synrepo/state/compact-state.json`
- **AND** subsequent `synrepo status` invocations display this timestamp
