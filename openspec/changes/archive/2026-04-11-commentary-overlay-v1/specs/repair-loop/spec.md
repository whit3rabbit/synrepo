## MODIFIED Requirements

### Requirement: Define targeted drift classes
synrepo SHALL classify repair findings as machine-readable drift records over named surfaces, including stale exports, stale runtime views, stale commentary overlay entries, broken declared links, stale rationale, trust conflicts, unsupported surfaces, and blocked repairs, so repair work can be scoped precisely and reported honestly. Cross-link review surfaces are a separate named surface that remains unsupported until `cross-link-overlay-v1` is implemented; they SHALL be reported as unsupported rather than conflated with commentary overlay staleness.

#### Scenario: Detect stale surfaces after repository changes
- **WHEN** project state diverges from declared intent or cached outputs and a user runs `synrepo check`
- **THEN** synrepo returns one drift finding per affected surface with its drift class, target identity, severity, and recommended repair action
- **AND** the system reports the issue without forcing a full rebuild of everything

#### Scenario: Report an unimplemented repair surface explicitly
- **WHEN** a repair surface named by the contract is not materialized or not implemented in the current runtime
- **THEN** synrepo reports that surface as unsupported or not applicable instead of silently skipping it
- **AND** the resulting report remains auditable about what was and was not checked

#### Scenario: Report cross-link review surface as unsupported
- **WHEN** `synrepo check` runs before `cross-link-overlay-v1` is implemented
- **THEN** synrepo reports the cross-link review surface as unsupported
- **AND** the report does not conflate the cross-link surface with the commentary overlay stale-entries surface
