## MODIFIED Requirements

### Requirement: Define targeted drift classes
synrepo SHALL classify repair findings as machine-readable drift records over named surfaces, including stale exports, stale runtime views, stale overlay entries, broken declared links, stale rationale, trust conflicts, unsupported surfaces, and blocked repairs, so repair work can be scoped precisely and reported honestly.

#### Scenario: Detect stale surfaces after repository changes
- **WHEN** project state diverges from declared intent or cached outputs and a user runs `synrepo check`
- **THEN** synrepo returns one drift finding per affected surface with its drift class, target identity, severity, and recommended repair action
- **AND** the system reports the issue without forcing a full rebuild of everything

#### Scenario: Report an unimplemented repair surface explicitly
- **WHEN** a repair surface named by the contract is not materialized or not implemented in the current runtime
- **THEN** synrepo reports that surface as unsupported or not applicable instead of silently skipping it
- **AND** the resulting report remains auditable about what was and was not checked

### Requirement: Define selective repair behavior
synrepo SHALL expose a read-only `check` workflow and a mutating `sync` workflow that repair only selected or auto-repairable deterministic surfaces while preserving graph versus overlay separation and reusing canonical producer paths.

#### Scenario: Refresh one selected stale surface
- **WHEN** a user requests repair for a specific stale surface through `synrepo sync`
- **THEN** synrepo refreshes only the targeted surface through its existing deterministic producer path
- **AND** it does not collapse canonical and supplemental stores into one trust bucket

#### Scenario: Encounter a report-only trust conflict during sync
- **WHEN** `synrepo sync` encounters a drift finding that requires human review rather than deterministic repair
- **THEN** synrepo records the finding as report-only and leaves the underlying surface unchanged
- **AND** the command output distinguishes skipped manual-review findings from repaired findings

### Requirement: Include exports and views in repair scope
synrepo SHALL treat generated exports and runtime views as repairable surfaces only when they declare freshness inputs and a deterministic refresh path, and targeted repair SHALL leave unrelated surfaces untouched.

#### Scenario: Repair a stale export
- **WHEN** a generated export or runtime view no longer matches its declared inputs and the user requests repair for that surface
- **THEN** the repair loop classifies and refreshes that surface specifically
- **AND** the repair does not require blanket regeneration of unrelated artifacts

#### Scenario: Check a repository with no materialized exports
- **WHEN** `synrepo check` runs in a repository that has no generated exports or runtime views for a declared repair surface
- **THEN** synrepo reports the surface as absent, unsupported, or not applicable according to the contract
- **AND** the lack of that surface does not produce a false successful repair result

### Requirement: Define CLI and CI repair surfaces
synrepo SHALL define `synrepo check` and `synrepo sync` as observable local CLI and CI repair surfaces, including machine-readable output, exit behavior, and append-only resolution logging for mutating runs.

#### Scenario: Run repair in CI
- **WHEN** a CI workflow runs `synrepo check`
- **THEN** the repair-loop contract defines the observable categories, machine-readable output, and exit behavior for clean, actionable, blocked, and unsupported findings
- **AND** the command remains read-only so CI can audit drift without mutating runtime state

#### Scenario: Audit a sync run
- **WHEN** a user or CI job runs `synrepo sync`
- **THEN** synrepo appends a resolution log entry containing the requested scope, findings considered, actions taken, and final outcome
- **AND** later operators can inspect what was detected and resolved without inferring it from silent background behavior
