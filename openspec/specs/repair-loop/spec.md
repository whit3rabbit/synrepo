## Purpose
Define targeted drift detection, selective repair, and auditability for stale synrepo surfaces.

## Requirements

### Requirement: Define targeted drift classes
synrepo SHALL define drift classes for stale exports, stale overlay entries, broken declared links, stale rationale, and trust conflicts so repair work can be scoped precisely.

#### Scenario: Detect stale surfaces after repository changes
- **WHEN** project state diverges from declared intent or cached outputs
- **THEN** the repair-loop spec defines the drift category that applies
- **AND** the system can report the issue without forcing a full rebuild of everything

### Requirement: Define selective repair behavior
synrepo SHALL define a repair workflow that checks current state against declared intent and repairs only the selected stale surfaces while preserving graph versus overlay separation.

#### Scenario: Refresh one stale overlay surface
- **WHEN** a user requests repair for a specific stale overlay item
- **THEN** the repair behavior refreshes only the targeted surface
- **AND** it does not collapse canonical and supplemental stores into one trust bucket

### Requirement: Include exports and views in repair scope
synrepo SHALL define generated exports and runtime views as repairable surfaces when their declared freshness or inputs become stale.

#### Scenario: Repair a stale export
- **WHEN** a generated export or runtime view no longer matches its inputs
- **THEN** the repair loop can classify and refresh that surface specifically
- **AND** the repair does not require blanket regeneration of unrelated artifacts

### Requirement: Define CLI and CI repair surfaces
synrepo SHALL define how targeted checking and syncing behavior appear in local CLI and CI flows, including resolution logging.

#### Scenario: Run repair in CI
- **WHEN** a CI workflow checks synrepo health
- **THEN** the repair-loop contract defines the observable categories and outputs
- **AND** it preserves auditability of detected and resolved issues
