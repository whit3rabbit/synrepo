## ADDED Requirements

### Requirement: Define export command contract
synrepo SHALL provide a `synrepo export` command that produces human-readable and machine-readable convenience snapshots of current card state. Exports SHALL be written to a configurable directory (default: `synrepo-context/` at the repo root) and SHALL never be used as synthesis input, graph truth, or canonical planning material.

#### Scenario: Run export on an initialized repository
- **WHEN** a user runs `synrepo export` in an initialized repository
- **THEN** synrepo writes one markdown file per card type (symbols, files, decisions) to `synrepo-context/` at `Normal` budget
- **AND** a `.export-manifest.json` records the graph schema version and last-reconcile timestamp used
- **AND** each export file contains a generated-file header that identifies it as a synrepo convenience output

#### Scenario: Run export with JSON format
- **WHEN** a user runs `synrepo export --format json`
- **THEN** synrepo writes a single `synrepo-context/index.json` with the same card content at `Normal` budget
- **AND** the manifest is updated to record the format and timestamp

#### Scenario: Run export with deep budget
- **WHEN** a user runs `synrepo export --deep`
- **THEN** synrepo uses `Deep` budget for each card, including commentary and cross-link fields where available
- **AND** the manifest records the budget tier used

#### Scenario: Export directory gitignore behavior
- **WHEN** `synrepo export` runs and `synrepo-context/` does not appear in any `.gitignore`
- **THEN** synrepo adds `synrepo-context/` to the repo-root `.gitignore`
- **AND** a `--commit` flag suppresses gitignore insertion when the user intends to track exports

### Requirement: Define export freshness derivation
synrepo SHALL derive export freshness by comparing the last-reconcile timestamp and graph schema version recorded in `.export-manifest.json` against the current runtime state. An export is fresh if the graph has not been reconciled since the manifest was written; stale if the graph advanced; or absent if no manifest exists.

#### Scenario: Check export freshness after a reconcile
- **WHEN** `synrepo check` runs and the graph has been reconciled after the last export
- **THEN** the `ExportSurface` is reported as stale with recommended action `regenerate_exports`
- **AND** the finding includes the export directory path and the timestamp delta

#### Scenario: Check export freshness on a fresh export
- **WHEN** `synrepo check` runs and the export manifest matches the current runtime epoch
- **THEN** the `ExportSurface` is reported as current
- **AND** no regeneration action is recommended

#### Scenario: Check export freshness with no exports
- **WHEN** `synrepo check` runs and no `synrepo-context/.export-manifest.json` exists
- **THEN** the `ExportSurface` is reported as absent
- **AND** no error is raised and the surface is not reported as unsupported
