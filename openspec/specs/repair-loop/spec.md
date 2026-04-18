## Purpose
Define targeted drift detection, selective repair, and auditability for stale synrepo surfaces.
## Requirements
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

### Requirement: Define cross-link revalidation as a repair action
synrepo SHALL define `revalidate_links` as a named repair action for the `proposed_links_overlay` surface. When stale cross-link candidates are detected by `synrepo check`, the recommended repair action SHALL be `revalidate_links`. When `synrepo sync` processes this action, it SHALL re-run the deterministic fuzzy-LCS verifier against each stale candidate's stored `CitedSpan`s using current endpoint source text; candidates whose spans still verify above threshold SHALL be refreshed with updated endpoint hashes and a new provenance timestamp, and candidates whose spans no longer verify SHALL be demoted to `below_threshold` with an audit-trail entry recording the demotion. `revalidate_links` SHALL NOT invoke the LLM generator; full regeneration requires an explicit `synrepo sync --regenerate-cross-links` path.

#### Scenario: Check detects stale cross-link candidates
- **WHEN** `synrepo check` runs and one or more `cross_links` rows have endpoint hashes that do not match the current graph
- **THEN** each stale candidate produces a drift finding with surface `proposed_links_overlay`, drift class `stale`, and recommended action `revalidate_links`
- **AND** the structural graph and other repair surfaces are reported independently

#### Scenario: Sync revalidates candidates whose evidence still holds
- **WHEN** `synrepo sync` processes a `revalidate_links` action for a stale candidate whose stored `CitedSpan`s still verify above the LCS threshold against current source text
- **THEN** the candidate is refreshed with updated endpoint hashes and a new provenance timestamp
- **AND** the candidate returns to `fresh` state without any LLM call

#### Scenario: Sync demotes candidates whose evidence no longer holds
- **WHEN** `synrepo sync` processes a `revalidate_links` action for a stale candidate whose stored `CitedSpan`s no longer verify above the LCS threshold
- **THEN** the candidate is demoted to `below_threshold` tier
- **AND** an audit row records the demotion with the previous tier, new tier, and timestamp
- **AND** the candidate is withheld from subsequent card responses until explicit regeneration

#### Scenario: Revalidation does not touch the graph
- **WHEN** `synrepo sync` processes a `revalidate_links` action
- **THEN** the graph store is not modified
- **AND** the structural compile is not triggered unless a separate structural finding is also being repaired

### Requirement: Define repair-log rotation as a compact sub-action
synrepo SHALL support repair-log rotation as a compact sub-action that summarizes JSONL entries older than the retention window into a header line while preserving recent entries.

#### Scenario: Detect compactable repair-log entries
- **WHEN** a compaction pass evaluates the repair-log
- **THEN** entries beyond the policy's retention window are identified for summarization
- **AND** recent entries are preserved verbatim

#### Scenario: Execute repair-log rotation
- **WHEN** rotation executes on a repair-log with entries beyond retention
- **THEN** old entries are summarized into a header line with counts by surface and action
- **AND** the log is rewritten atomically with the header followed by retained entries
- **AND** the file ends with `.jsonl` extension (no `.tmp` artifacts)

#### Scenario: Repair-log rotation is idempotent
- **WHEN** rotation runs on an already-compacted log
- **THEN** no entries are summarized (counts are zero)
- **AND** the existing header is preserved

### Requirement: Define retired observations as a repair surface
synrepo SHALL define `retired_observations` as a named repair surface for graph facts that have been soft-retired but not yet compacted. When the count of retired symbols or edges exceeds a reporting threshold, `synrepo check` SHALL report the surface with recommended action `compact_retired`. When `synrepo sync` processes this action, it SHALL run the compaction pass using the configured retention window.

#### Scenario: Check reports retired observation accumulation
- **WHEN** `synrepo check` runs and the graph contains retired symbols or edges
- **THEN** a drift finding is produced with surface `retired_observations`, drift class `current`, and the count of retired facts
- **AND** when the count exceeds the reporting threshold, the recommended action is `compact_retired`

#### Scenario: Sync compacts retired observations
- **WHEN** `synrepo sync` processes a `compact_retired` action
- **THEN** synrepo runs the compaction pass for retired facts older than `retain_retired_revisions`
- **AND** the resolution log records the number of symbols, edges, and sidecar rows removed
- **AND** the graph store is modified only by removing retired facts, not active observations

#### Scenario: Compaction does not affect other repair surfaces
- **WHEN** `synrepo sync` processes a `compact_retired` action
- **THEN** the overlay store is not modified
- **AND** other repair surfaces (exports, commentary, proposed links) are reported independently

### Requirement: Cross-link span verification handles large sources
Cross-link verification in `src/pipeline/repair/cross_link_verify.rs` SHALL use a three-stage cascade to verify cited spans: (1) exact substring match on normalized text for verbatim citations, (2) anchored partial match with LCS verification for paraphrases, and (3) budgeted windowed LCS fallback. This enables verification of spans in sources larger than 4KB without silent drops.

#### Scenario: Verify verbatim citation in 10KB source
- **GIVEN** a cross-link with a cited span of 200 bytes in a source file of 10KB
- **WHEN** `verify_candidate_payload` validates the link
- **THEN** Stage A (exact substring) finds the span immediately with ratio = 1.0
- **AND** the function returns a `CitedSpan` with `lcs_ratio = 1.0` and `verified_at_offset` pointing to the found location

#### Scenario: Verify paraphrase in large source
- **GIVEN** a cross-link with a cited span that differs from the source by one word
- **WHEN** Stage A misses (not exact match) but Stage B anchor is found
- **THEN** Stage B evaluates LCS on a window around each anchor hit
- **AND** returns ratio >= 0.9 if found, otherwise falls through to Stage C

#### Scenario: Handle pathological large source with budget
- **GIVEN** a 500KB source with a needle that doesn't match
- **WHEN** stages A and B both fail to find a >= 0.9 ratio match
- **THEN** Stage C runs with a 50ms time budget
- **AND** returns the best-so-far ratio found before budget trip, or None if none evaluated
- **AND** emits a `tracing::warn!` with source length, needle length, and best ratio

### Requirement: Logging for verification stage decisions
The cross-link verification path SHALL emit structured logging to enable observability of which stage in the cascade produced a match and when budget trips occur.

#### Scenario: Log stage cascade decision
- **GIVEN** a span verification request
- **WHEN** a match is found in any stage
- **THEN** emit `tracing::debug!(stage = "A"|"B"|"C", ratio, message)` with the stage identifier

#### Scenario: Log budget trip
- **GIVEN** a verification that exceeds the time budget in Stage B or Stage C
- **WHEN** the budget check triggers
- **THEN** emit `tracing::warn!` with stage, source length, needle length, anchor hits (Stage B), iterations (Stage C), and best ratio if found

