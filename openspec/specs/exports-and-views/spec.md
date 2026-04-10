## Purpose
Define generated exports and runtime views as convenience surfaces that remain subordinate to graph truth and are never used as canonical synthesis input.

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
synrepo SHALL prevent generated exports and views from becoming synthesis input or graph truth unless they are separately promoted through human-authored source material.

#### Scenario: Reuse generated output during synthesis
- **WHEN** a synthesis or retrieval pipeline encounters generated export material
- **THEN** the pipeline excludes it from canonical graph input and default synthesis input
- **AND** any later promotion requires an explicit human-authored source path
