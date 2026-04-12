## ADDED Requirements

### Requirement: Define commentary refresh as a repair action
synrepo SHALL define `refresh_commentary` as a named repair action for the `commentary_overlay_entries` surface. When stale commentary entries are detected by `synrepo check`, the recommended repair action SHALL be `refresh_commentary`. When `synrepo sync` processes this action, it SHALL trigger the commentary generator for each stale entry within the configured cost limit and update the overlay store with the refreshed entries and new provenance.

#### Scenario: Check detects stale commentary entries
- **WHEN** `synrepo check` runs and one or more commentary entries have a `source_content_hash` that does not match the current graph
- **THEN** each stale entry produces a drift finding with surface `commentary_overlay_entries`, drift class `stale`, and recommended action `refresh_commentary`
- **AND** the structural graph and other repair surfaces are reported independently

#### Scenario: Sync refreshes stale commentary entries within budget
- **WHEN** `synrepo sync` processes a `refresh_commentary` action
- **THEN** the commentary generator runs for each stale entry, up to the configured cost limit
- **AND** entries updated within budget are re-persisted with fresh provenance
- **AND** entries skipped due to budget exhaustion are reported as `blocked` in the resolution log

#### Scenario: Commentary refresh does not touch the graph store
- **WHEN** `synrepo sync` refreshes stale commentary
- **THEN** the graph store is not modified
- **AND** the structural compile is not triggered unless a separate structural finding is also being repaired

### Requirement: Report commentary overlay entries surface as active, not unsupported
synrepo SHALL report the `commentary_overlay_entries` repair surface as an active surface once the overlay store is initialized. Prior to initialization (no `.synrepo/overlay/overlay.db`), the surface SHALL be reported as `absent` rather than `unsupported`.

#### Scenario: Commentary overlay surface is active after first use
- **WHEN** `synrepo check` runs and `.synrepo/overlay/overlay.db` exists
- **THEN** the `commentary_overlay_entries` surface is checked and reported as `current`, `stale`, or `absent` (no entries yet)
- **AND** the surface is not reported as `unsupported`

#### Scenario: Commentary overlay surface is absent before first use
- **WHEN** `synrepo check` runs and `.synrepo/overlay/overlay.db` does not exist
- **THEN** the `commentary_overlay_entries` surface is reported as `absent`
- **AND** no error is raised
