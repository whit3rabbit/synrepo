## Purpose
Define generated exports and runtime views as convenience surfaces that remain subordinate to graph truth and are never used as canonical explain input.

## Requirements

### Requirement: Distinguish runtime views from exports
synrepo SHALL distinguish internal runtime views from explicit exports so generated material does not silently become part of the canonical planning or retrieval surface.

#### Scenario: Produce generated project output
- **WHEN** synrepo emits generated material derived from graph or overlay state
- **THEN** the contract identifies whether the output is a runtime view or an export
- **AND** the output is labeled as a convenience surface rather than source-of-truth state

### Requirement: Define export freshness and repair behavior
synrepo SHALL define freshness labeling, stale-state handling, and repair-loop participation for generated exports and views.

#### Scenario: Detect a stale generated export
- **WHEN** underlying graph or overlay inputs change after an export is produced
- **THEN** synrepo can identify the export as stale and eligible for targeted repair
- **AND** the repair behavior does not require a full system rebuild

### Requirement: Prevent export feedback contamination
synrepo SHALL prevent generated exports and views from becoming explain input or graph truth unless they are separately promoted through human-authored source material.

#### Scenario: Reuse generated output during explain
- **WHEN** an explain or retrieval pipeline encounters generated export material
- **THEN** the pipeline excludes it from canonical graph input and default explain input
- **AND** any later promotion requires an explicit human-authored source path

### Requirement: Define export command contract
synrepo SHALL provide a `synrepo export` command that produces human-readable and machine-readable convenience snapshots of current card state. Exports SHALL be written to a configurable directory (default: `synrepo-context/` at the repo root) and SHALL never be used as explain input, graph truth, or canonical planning material.

#### Scenario: Run export on an initialized repository
- **WHEN** a user runs `synrepo export` in an initialized repository
- **THEN** synrepo writes one markdown file per card type (symbols, files, decisions) to `synrepo-context/` at `Normal` budget
- **AND** a `.export-manifest.json` records the graph schema version and last-reconcile timestamp used
- **AND** each export file contains a generated-file header that identifies it as a synrepo convenience output

#### Scenario: Run export with JSON format
- **WHEN** a user runs `synrepo export --format json`
- **THEN** synrepo writes a single `synrepo-context/index.json` with the same card content at `Normal` budget
- **AND** the manifest is updated to record the format and timestamp

#### Scenario: Run export with graph JSON format
- **WHEN** a user runs `synrepo export --format graph-json`
- **THEN** synrepo writes `synrepo-context/graph.json` with active canonical graph nodes and edges
- **AND** exported records include provenance, epistemic labels, edge kinds, degree counts, and drift scores when available
- **AND** no explain provider, API key, or overlay read is required

#### Scenario: Run export with graph HTML format
- **WHEN** a user runs `synrepo export --format graph-html`
- **THEN** synrepo writes a self-contained `synrepo-context/graph.html` and a matching `synrepo-context/graph.json`
- **AND** the HTML view supports search, node-type filtering, edge-kind filtering, degree filtering, node inspection, and neighborhood expansion without loading external assets

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
