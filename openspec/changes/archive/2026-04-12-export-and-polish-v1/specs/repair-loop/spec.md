## ADDED Requirements

### Requirement: Define ExportSurface as a repair surface
synrepo SHALL define `ExportSurface` as a named repair surface for generated exports written by `synrepo export`. When generated exports are stale (the graph has advanced since the last export), `synrepo check` SHALL report the surface as stale with recommended action `regenerate_exports`. When `synrepo sync` processes this action, it SHALL re-run `synrepo export` with the same format and budget options recorded in the export manifest.

#### Scenario: Check detects stale exports
- **WHEN** `synrepo check` runs and the export manifest's recorded reconcile epoch is older than the current runtime epoch
- **THEN** a drift finding is produced with surface `export_surface`, drift class `stale`, and recommended action `regenerate_exports`
- **AND** the finding includes the export directory path and manifest timestamp

#### Scenario: Sync regenerates stale exports
- **WHEN** `synrepo sync` processes a `regenerate_exports` action
- **THEN** synrepo re-runs the export command using the format and budget tier recorded in the manifest
- **AND** the manifest is updated with the new reconcile epoch
- **AND** the graph store and overlay store are not modified

#### Scenario: Check reports absent exports as absent, not stale
- **WHEN** `synrepo check` runs and no export manifest exists
- **THEN** the `export_surface` is reported as absent
- **AND** no regeneration action is recommended and no error is raised

#### Scenario: Export repair does not touch the graph
- **WHEN** `synrepo sync` processes a `regenerate_exports` action
- **THEN** the graph store is not modified
- **AND** the structural compile is not triggered unless a separate structural finding is also being repaired

## MODIFIED Requirements

### Requirement: Define targeted drift classes
synrepo SHALL classify repair findings as machine-readable drift records over named surfaces, including stale exports, stale runtime views, stale commentary overlay entries, stale proposed-links overlay entries, broken declared links, stale rationale, trust conflicts, unsupported surfaces, and blocked repairs, so repair work can be scoped precisely and reported honestly. The `proposed_links_overlay` surface SHALL be reported as an active surface once the overlay store contains any `cross_links` rows; prior to that, the surface SHALL be reported as `absent` rather than `unsupported`. The `export_surface` SHALL be reported as absent when no export manifest exists and as stale when the manifest epoch is behind the current runtime epoch.

#### Scenario: Detect stale surfaces after repository changes
- **WHEN** project state diverges from declared intent or cached outputs and a user runs `synrepo check`
- **THEN** synrepo returns one drift finding per affected surface with its drift class, target identity, severity, and recommended repair action
- **AND** the system reports the issue without forcing a full rebuild of everything

#### Scenario: Report an unimplemented repair surface explicitly
- **WHEN** a repair surface named by the contract is not materialized or not implemented in the current runtime
- **THEN** synrepo reports that surface as unsupported or not applicable instead of silently skipping it
- **AND** the resulting report remains auditable about what was and was not checked

#### Scenario: Report proposed-links overlay surface as absent before first use
- **WHEN** `synrepo check` runs and the `cross_links` table is empty
- **THEN** synrepo reports the `proposed_links_overlay` surface as `absent`
- **AND** no error is raised

#### Scenario: Report proposed-links overlay surface as stale after source edits
- **WHEN** `synrepo check` runs and one or more `cross_links` rows have an endpoint whose current `FileNode.content_hash` differs from the stored hash
- **THEN** each stale candidate produces a drift finding with surface `proposed_links_overlay`, drift class `stale`, and recommended action `revalidate_links`
- **AND** findings for deleted endpoints are reported separately with drift class `source_deleted` and recommended action `manual_review`

#### Scenario: Report export surface as stale after reconcile
- **WHEN** `synrepo check` runs and the export manifest epoch is behind the current runtime epoch
- **THEN** synrepo reports the `export_surface` as stale with recommended action `regenerate_exports`
- **AND** the finding is independent of other surface findings
