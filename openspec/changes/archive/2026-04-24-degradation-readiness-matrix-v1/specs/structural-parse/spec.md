## ADDED Requirements

### Requirement: Map parser coverage failures to readiness
Structural parse diagnostics SHALL map parser failures and unsupported-language gaps into capability readiness rows.

#### Scenario: Parser failures occur during reconcile
- **WHEN** a reconcile or bootstrap pass records parser failures for supported files
- **THEN** the readiness matrix marks parser coverage as degraded
- **AND** the row includes failure counts and a next action that points to check, sync, or parser diagnostics

#### Scenario: Unsupported files are present
- **WHEN** files are unsupported by structural parsing but are otherwise admitted by the repo
- **THEN** the readiness matrix distinguishes unsupported coverage from parser failure
- **AND** unsupported coverage does not masquerade as parser-observed graph truth
