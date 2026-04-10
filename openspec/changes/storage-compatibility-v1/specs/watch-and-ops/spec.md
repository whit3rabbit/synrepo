## MODIFIED Requirements

### Requirement: Define store retention and compatibility operations
synrepo SHALL provide operational maintenance behavior that applies the declared retention, cleanup, rebuild, and migration rules consistently across runtime stores.

#### Scenario: Run maintenance after storage drift
- **WHEN** a user or automated flow performs runtime maintenance after stores age out, exceed retention limits, or become incompatible
- **THEN** the maintenance behavior applies the declared storage classes and compatibility policy
- **AND** the outcome is observable rather than being hidden background cleanup

### Requirement: Define operational diagnostics
synrepo SHALL expose enough operational diagnostics to explain storage compatibility state, retention pressure, and required maintenance actions.

#### Scenario: Diagnose an incompatible runtime store
- **WHEN** a command encounters an incompatible or retention-expired runtime store
- **THEN** synrepo reports the relevant storage state and required action clearly
- **AND** the user is not left with a generic operational failure
