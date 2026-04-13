## MODIFIED Requirements

### Requirement: Define targeted drift classes
synrepo SHALL classify repair findings as machine-readable drift records over named surfaces, including stale exports, stale runtime views, stale commentary overlay entries, stale proposed-links overlay entries, broken declared links, stale rationale, trust conflicts, unsupported surfaces, and blocked repairs, so repair work can be scoped precisely and reported honestly. The `proposed_links_overlay` surface SHALL be reported as an active surface once the overlay store contains any `cross_links` rows; prior to that, the surface SHALL be reported as `absent` rather than `unsupported`.

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

## ADDED Requirements

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
